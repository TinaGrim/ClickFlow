use crate::model::{
    ClickerConfig, MacroAction, MacroConfig, MouseButtonChoice, ScrollDirection, TargetMode,
};
use enigo::{
    Axis, Button, Coordinate,
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const REPORT_INTERVAL: Duration = Duration::from_millis(33);
const MACRO_CLICK_HOLD: Duration = Duration::from_millis(12);

#[derive(Clone, Debug)]
pub enum RunConfig {
    Clicker(ClickerConfig),
    Macro(MacroConfig),
}

impl RunConfig {
    pub fn total(&self) -> Option<u64> {
        match self {
            Self::Clicker(config) => config.total_clicks(),
            Self::Macro(config) => config.total_passes(),
        }
    }

    pub fn start_delay(&self) -> f64 {
        match self {
            Self::Clicker(config) => config.start_delay,
            Self::Macro(config) => config.start_delay,
        }
    }
}

#[derive(Clone, Debug)]
pub enum EngineEvent {
    Countdown {
        remaining_secs: f64,
    },
    Started {
        total: Option<u64>,
    },
    Progress {
        completed: u64,
        total: Option<u64>,
    },
    Finished {
        completed: u64,
        total: Option<u64>,
        stopped: bool,
        error: Option<String>,
    },
}

pub struct EngineHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    receiver: Receiver<EngineEvent>,
}

impl EngineHandle {
    pub fn spawn(config: RunConfig) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (sender, receiver) = mpsc::channel();

        let join = thread::Builder::new()
            .name("clickflow-engine".to_owned())
            .spawn(move || {
                let panic_sender = sender.clone();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    run_engine(config, &worker_stop, &sender);
                }));
                if let Err(payload) = result {
                    let message = panic_message(payload);
                    let _ = panic_sender.send(EngineEvent::Finished {
                        completed: 0,
                        total: None,
                        stopped: false,
                        error: Some(format!("The click engine stopped unexpectedly: {message}")),
                    });
                }
            })
            .map_err(|error| format!("Could not start the click engine: {error}"))?;

        Ok(Self {
            stop,
            join: Some(join),
            receiver,
        })
    }

    pub fn try_recv(&self) -> Result<EngineEvent, TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn stop(&mut self) {
        self.request_stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Outcome {
    completed: u64,
    total: Option<u64>,
    stopped: bool,
    error: Option<String>,
}

impl Outcome {
    fn stopped(completed: u64, total: Option<u64>) -> Self {
        Self {
            completed,
            total,
            stopped: true,
            error: None,
        }
    }

    fn complete(completed: u64, total: Option<u64>) -> Self {
        Self {
            completed,
            total,
            stopped: false,
            error: None,
        }
    }

    fn failed(completed: u64, total: Option<u64>, error: String) -> Self {
        Self {
            completed,
            total,
            stopped: false,
            error: Some(error),
        }
    }
}

fn run_engine(config: RunConfig, stop: &AtomicBool, sender: &Sender<EngineEvent>) {
    let config = match config {
        RunConfig::Clicker(clicker) => RunConfig::Clicker(clicker.sanitized()),
        RunConfig::Macro(macro_flow) => RunConfig::Macro(macro_flow.sanitized()),
    };
    let total = config.total();
    let start_delay = config.start_delay();

    let settings = Settings::default();
    let mut enigo = match Enigo::new(&settings) {
        Ok(enigo) => enigo,
        Err(error) => {
            let _ = sender.send(EngineEvent::Finished {
                completed: 0,
                total,
                stopped: false,
                error: Some(enigo_setup_error(&error.to_string())),
            });
            return;
        }
    };

    if !wait_before_start(start_delay, stop, sender) {
        let _ = sender.send(EngineEvent::Finished {
            completed: 0,
            total,
            stopped: true,
            error: None,
        });
        return;
    }

    let _ = sender.send(EngineEvent::Started { total });

    let outcome = match config {
        RunConfig::Clicker(clicker) => run_clicker(&mut enigo, clicker, stop, sender),
        RunConfig::Macro(macro_flow) => run_macro(&mut enigo, macro_flow, stop, sender),
    };

    let _ = sender.send(EngineEvent::Finished {
        completed: outcome.completed,
        total: outcome.total,
        stopped: outcome.stopped,
        error: outcome.error,
    });
}

fn run_clicker(
    enigo: &mut Enigo,
    config: ClickerConfig,
    stop: &AtomicBool,
    sender: &Sender<EngineEvent>,
) -> Outcome {
    let total = config.total_clicks();
    let interval = Duration::from_secs_f64(config.effective_interval_secs());
    let button = enigo_button(config.button);
    let mut completed = 0_u64;
    let mut last_report = Instant::now() - Duration::from_secs(1);
    let mut reached_limit = false;

    while !stop.load(Ordering::Relaxed) {
        let cycle_start = Instant::now();

        if config.target == TargetMode::Fixed
            && let Err(error) = enigo.move_mouse(config.x, config.y, Coordinate::Abs)
        {
            return Outcome::failed(
                completed,
                total,
                format!("Could not move the cursor: {error}"),
            );
        }

        if !sleep_interruptible(config.before_click, stop) {
            break;
        }

        if let Err(error) = click_once(enigo, button, Duration::from_secs_f64(config.hold_time)) {
            return Outcome::failed(completed, total, error);
        }

        completed = completed.saturating_add(1);

        let reached = total.is_some_and(|limit| completed >= limit);
        report_progress(sender, completed, total, &mut last_report, reached);
        if reached {
            reached_limit = true;
            break;
        }

        if !sleep_interruptible(config.after_click, stop) {
            break;
        }

        let next_cycle = cycle_start + interval;
        if !wait_until(next_cycle, stop) {
            break;
        }
    }

    if reached_limit {
        Outcome::complete(completed, total)
    } else {
        Outcome::stopped(completed, total)
    }
}

fn run_macro(
    enigo: &mut Enigo,
    config: MacroConfig,
    stop: &AtomicBool,
    sender: &Sender<EngineEvent>,
) -> Outcome {
    let total = config.total_passes();
    if config.steps.is_empty() {
        return Outcome::failed(
            0,
            total,
            "This macro has no steps. Add at least one step before starting.".to_owned(),
        );
    }

    let mut completed = 0_u64;
    let mut last_report = Instant::now() - Duration::from_secs(1);
    let mut reached_limit = false;

    'passes: loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        for step in &config.steps {
            if stop.load(Ordering::Relaxed) {
                break 'passes;
            }
            if let Err(error) = execute_step(enigo, &step.action) {
                return Outcome::failed(completed, total, error);
            }
            if !sleep_interruptible(step.delay_after, stop) {
                break 'passes;
            }
        }

        completed = completed.saturating_add(1);
        let reached = total.is_some_and(|limit| completed >= limit);
        report_progress(sender, completed, total, &mut last_report, reached);
        if reached {
            reached_limit = true;
            break;
        }

        if !sleep_interruptible(config.loop_delay, stop) {
            break;
        }
    }

    if reached_limit {
        Outcome::complete(completed, total)
    } else {
        Outcome::stopped(completed, total)
    }
}

fn execute_step(enigo: &mut Enigo, action: &MacroAction) -> Result<(), String> {
    match action {
        MacroAction::Click {
            target,
            x,
            y,
            button,
        } => {
            if *target == TargetMode::Fixed {
                enigo
                    .move_mouse(*x, *y, Coordinate::Abs)
                    .map_err(|error| format!("Could not move the cursor: {error}"))?;
            }
            click_once(enigo, enigo_button(*button), MACRO_CLICK_HOLD)
        }
        MacroAction::Move { x, y } => enigo
            .move_mouse(*x, *y, Coordinate::Abs)
            .map_err(|error| format!("Could not move the cursor: {error}")),
        MacroAction::Pause => Ok(()),
        MacroAction::Text { text } => {
            if text.is_empty() {
                Ok(())
            } else {
                enigo
                    .text(text)
                    .map_err(|error| format!("Could not type text: {error}"))
            }
        }
        MacroAction::KeyPress { key, modifiers } => {
            let key = parse_key(key).ok_or_else(|| {
                format!(
                    "Unknown key \"{key}\". Use a single character, enter, tab, esc, space, arrows, home, end, pageup, pagedown, or f1–f12."
                )
            })?;
            press_key_combo(enigo, key, *modifiers)
        }
        MacroAction::Scroll { amount, direction } => {
            let axis = match direction {
                ScrollDirection::Vertical => Axis::Vertical,
                ScrollDirection::Horizontal => Axis::Horizontal,
            };
            enigo
                .scroll(*amount, axis)
                .map_err(|error| format!("Could not scroll: {error}"))
        }
    }
}

fn click_once(enigo: &mut Enigo, button: Button, hold: Duration) -> Result<(), String> {
    enigo
        .button(button, Press)
        .map_err(|error| format!("Could not press the mouse button: {error}"))?;
    if !hold.is_zero() {
        thread::sleep(hold);
    }
    enigo
        .button(button, Release)
        .map_err(|error| format!("Could not release the mouse button: {error}"))
}

fn press_key_combo(
    enigo: &mut Enigo,
    key: Key,
    modifiers: crate::model::ModifierKeys,
) -> Result<(), String> {
    let mut modifier_keys = Vec::new();
    if modifiers.control {
        modifier_keys.push(Key::Control);
    }
    if modifiers.shift {
        modifier_keys.push(Key::Shift);
    }
    if modifiers.alt {
        modifier_keys.push(Key::Alt);
    }
    if modifiers.meta {
        modifier_keys.push(Key::Meta);
    }

    let mut pressed = Vec::new();
    for modifier in modifier_keys {
        if let Err(error) = enigo.key(modifier, Press) {
            for pressed_modifier in pressed.into_iter().rev() {
                let _ = enigo.key(pressed_modifier, Release);
            }
            return Err(format!("Could not press a modifier key: {error}"));
        }
        pressed.push(modifier);
    }

    let result = enigo.key(key, Click);
    for modifier in pressed.into_iter().rev() {
        let _ = enigo.key(modifier, Release);
    }
    result.map_err(|error| format!("Could not press the key: {error}"))
}

pub fn cursor_position() -> Result<(i32, i32), String> {
    let enigo =
        Enigo::new(&Settings::default()).map_err(|error| enigo_setup_error(&error.to_string()))?;
    enigo
        .location()
        .map_err(|error| format!("Could not read the cursor position: {error}"))
}

pub fn parse_key(input: &str) -> Option<Key> {
    let trimmed = input.trim();
    if trimmed.chars().count() == 1 {
        return trimmed.chars().next().map(Key::Unicode);
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "enter" | "return" => Some(Key::Return),
        "tab" => Some(Key::Tab),
        "esc" | "escape" => Some(Key::Escape),
        "space" | "spacebar" => Some(Key::Space),
        "backspace" | "back" => Some(Key::Backspace),
        "delete" | "del" => Some(Key::Delete),
        "up" | "arrowup" => Some(Key::UpArrow),
        "down" | "arrowdown" => Some(Key::DownArrow),
        "left" | "arrowleft" => Some(Key::LeftArrow),
        "right" | "arrowright" => Some(Key::RightArrow),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" | "pgup" => Some(Key::PageUp),
        "pagedown" | "pgdn" => Some(Key::PageDown),
        "f1" => Some(Key::F1),
        "f2" => Some(Key::F2),
        "f3" => Some(Key::F3),
        "f4" => Some(Key::F4),
        "f5" => Some(Key::F5),
        "f6" => Some(Key::F6),
        "f7" => Some(Key::F7),
        "f8" => Some(Key::F8),
        "f9" => Some(Key::F9),
        "f10" => Some(Key::F10),
        "f11" => Some(Key::F11),
        "f12" => Some(Key::F12),
        _ => None,
    }
}

fn enigo_button(button: MouseButtonChoice) -> Button {
    match button {
        MouseButtonChoice::Left => Button::Left,
        MouseButtonChoice::Right => Button::Right,
        MouseButtonChoice::Middle => Button::Middle,
    }
}

fn enigo_setup_error(message: &str) -> String {
    if cfg!(target_os = "macos") {
        format!(
            "Could not take control of the mouse ({message}). Enable ClickFlow in System Settings › Privacy & Security › Accessibility. When running from source, enable your terminal app instead."
        )
    } else {
        format!("Could not take control of the mouse: {message}")
    }
}

fn wait_before_start(seconds: f64, stop: &AtomicBool, sender: &Sender<EngineEvent>) -> bool {
    if seconds <= 0.0 {
        return !stop.load(Ordering::Relaxed);
    }
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    let mut next_countdown_update = Instant::now();

    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        if now >= next_countdown_update {
            let remaining = deadline.saturating_duration_since(now).as_secs_f64();
            let _ = sender.send(EngineEvent::Countdown {
                remaining_secs: remaining,
            });
            next_countdown_update = now + Duration::from_millis(100);
        }
        thread::sleep((deadline - now).min(Duration::from_millis(20)));
    }
}

fn sleep_interruptible(seconds: f64, stop: &AtomicBool) -> bool {
    if seconds <= 0.0 {
        return !stop.load(Ordering::Relaxed);
    }
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    wait_until(deadline, stop)
}

fn wait_until(deadline: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        let remaining = deadline - now;
        thread::sleep(remaining.min(Duration::from_millis(2)));
    }
}

fn report_progress(
    sender: &Sender<EngineEvent>,
    completed: u64,
    total: Option<u64>,
    last_report: &mut Instant,
    force: bool,
) {
    if force || last_report.elapsed() >= REPORT_INTERVAL {
        let _ = sender.send(EngineEvent::Progress { completed, total });
        *last_report = Instant::now();
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_are_parsed() {
        assert_eq!(parse_key("enter"), Some(Key::Return));
        assert_eq!(parse_key("ESC"), Some(Key::Escape));
        assert_eq!(parse_key("f12"), Some(Key::F12));
        assert_eq!(parse_key("a"), Some(Key::Unicode('a')));
        assert_eq!(parse_key("not-a-key"), None);
    }

    #[test]
    fn zero_delay_respects_stop_flag() {
        let stop = AtomicBool::new(false);
        assert!(sleep_interruptible(0.0, &stop));
        stop.store(true, Ordering::SeqCst);
        assert!(!sleep_interruptible(0.0, &stop));
    }
}
