use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabKind {
    #[default]
    Clicker,
    Macro,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RateUnit {
    #[default]
    PerSecond,
    PerMinute,
}

impl RateUnit {
    pub fn short_label(self) -> &'static str {
        match self {
            Self::PerSecond => "sec",
            Self::PerMinute => "min",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TargetMode {
    #[default]
    Cursor,
    Fixed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseButtonChoice {
    #[default]
    Left,
    Right,
    Middle,
}

impl MouseButtonChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Middle => "Middle",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollDirection {
    #[default]
    Vertical,
    Horizontal,
}

impl ScrollDirection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vertical => "Vertical",
            Self::Horizontal => "Horizontal",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ClickerConfig {
    pub rate: f64,
    pub rate_unit: RateUnit,
    pub loop_forever: bool,
    pub repeat_count: u64,
    pub start_delay: f64,
    pub before_click: f64,
    pub hold_time: f64,
    pub after_click: f64,
    pub target: TargetMode,
    pub x: i32,
    pub y: i32,
    pub button: MouseButtonChoice,
}

impl Default for ClickerConfig {
    fn default() -> Self {
        Self {
            rate: 10.0,
            rate_unit: RateUnit::PerSecond,
            loop_forever: true,
            repeat_count: 100,
            start_delay: 1.0,
            before_click: 0.0,
            hold_time: 0.01,
            after_click: 0.0,
            target: TargetMode::Cursor,
            x: 0,
            y: 0,
            button: MouseButtonChoice::Left,
        }
    }
}

impl ClickerConfig {
    pub fn sanitized(mut self) -> Self {
        self.rate = finite_or(self.rate, 10.0).clamp(0.01, 1_000_000.0);
        self.repeat_count = self.repeat_count.max(1);
        self.start_delay = finite_or(self.start_delay, 0.0).clamp(0.0, 86_400.0);
        self.before_click = finite_or(self.before_click, 0.0).clamp(0.0, 86_400.0);
        self.hold_time = finite_or(self.hold_time, 0.01).clamp(0.0, 5.0);
        self.after_click = finite_or(self.after_click, 0.0).clamp(0.0, 86_400.0);
        self.x = self.x.clamp(-100_000, 100_000);
        self.y = self.y.clamp(-100_000, 100_000);
        self
    }

    pub fn base_interval_secs(&self) -> f64 {
        let rate = finite_or(self.rate, 10.0).clamp(0.01, 1_000_000.0);
        match self.rate_unit {
            RateUnit::PerSecond => 1.0 / rate,
            RateUnit::PerMinute => 60.0 / rate,
        }
    }

    pub fn minimum_cycle_secs(&self) -> f64 {
        finite_or(self.before_click, 0.0)
            + finite_or(self.hold_time, 0.0)
            + finite_or(self.after_click, 0.0)
    }

    pub fn effective_interval_secs(&self) -> f64 {
        self.base_interval_secs()
            .max(self.minimum_cycle_secs())
            .max(0.001)
    }

    pub fn effective_cps(&self) -> f64 {
        1.0 / self.effective_interval_secs()
    }

    pub fn total_clicks(&self) -> Option<u64> {
        if self.loop_forever {
            None
        } else {
            Some(self.repeat_count.max(1))
        }
    }

    pub fn estimated_total_secs(&self) -> Option<f64> {
        self.total_clicks()
            .map(|count| self.start_delay + count as f64 * self.effective_interval_secs())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModifierKeys {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum MacroAction {
    Click {
        target: TargetMode,
        x: i32,
        y: i32,
        button: MouseButtonChoice,
    },
    Move {
        x: i32,
        y: i32,
    },
    Pause,
    Text {
        text: String,
    },
    KeyPress {
        key: String,
        modifiers: ModifierKeys,
    },
    Scroll {
        amount: i32,
        direction: ScrollDirection,
    },
}

impl Default for MacroAction {
    fn default() -> Self {
        Self::Click {
            target: TargetMode::Cursor,
            x: 0,
            y: 0,
            button: MouseButtonChoice::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Click,
    Move,
    Pause,
    Text,
    KeyPress,
    Scroll,
}

impl ActionKind {
    pub const ALL: [Self; 6] = [
        Self::Click,
        Self::Move,
        Self::Pause,
        Self::Text,
        Self::KeyPress,
        Self::Scroll,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::Move => "Move",
            Self::Pause => "Pause",
            Self::Text => "Type text",
            Self::KeyPress => "Key press",
            Self::Scroll => "Scroll",
        }
    }

    pub fn add_label(self) -> &'static str {
        match self {
            Self::Click => "+ Click",
            Self::Move => "+ Move",
            Self::Pause => "+ Pause",
            Self::Text => "+ Text",
            Self::KeyPress => "+ Key",
            Self::Scroll => "+ Scroll",
        }
    }
}

impl MacroAction {
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Click { .. } => ActionKind::Click,
            Self::Move { .. } => ActionKind::Move,
            Self::Pause => ActionKind::Pause,
            Self::Text { .. } => ActionKind::Text,
            Self::KeyPress { .. } => ActionKind::KeyPress,
            Self::Scroll { .. } => ActionKind::Scroll,
        }
    }

    pub fn default_for(kind: ActionKind) -> Self {
        match kind {
            ActionKind::Click => Self::default(),
            ActionKind::Move => Self::Move { x: 0, y: 0 },
            ActionKind::Pause => Self::Pause,
            ActionKind::Text => Self::Text {
                text: String::new(),
            },
            ActionKind::KeyPress => Self::KeyPress {
                key: "enter".to_owned(),
                modifiers: ModifierKeys::default(),
            },
            ActionKind::Scroll => Self::Scroll {
                amount: 3,
                direction: ScrollDirection::Vertical,
            },
        }
    }

    pub fn default_delay_after(kind: ActionKind) -> f64 {
        match kind {
            ActionKind::Pause => 1.0,
            _ => 0.1,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct MacroStep {
    pub action: MacroAction,
    pub delay_after: f64,
}

impl Default for MacroStep {
    fn default() -> Self {
        Self {
            action: MacroAction::default(),
            delay_after: 0.1,
        }
    }
}

impl MacroStep {
    pub fn new(kind: ActionKind) -> Self {
        Self {
            action: MacroAction::default_for(kind),
            delay_after: MacroAction::default_delay_after(kind),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct MacroConfig {
    pub loop_forever: bool,
    pub repeat_count: u64,
    pub start_delay: f64,
    pub loop_delay: f64,
    pub steps: Vec<MacroStep>,
}

impl Default for MacroConfig {
    fn default() -> Self {
        Self {
            loop_forever: false,
            repeat_count: 1,
            start_delay: 1.0,
            loop_delay: 0.0,
            steps: Vec::new(),
        }
    }
}

impl MacroConfig {
    pub fn sanitized(mut self) -> Self {
        self.repeat_count = self.repeat_count.max(1);
        self.start_delay = finite_or(self.start_delay, 0.0).clamp(0.0, 86_400.0);
        self.loop_delay = finite_or(self.loop_delay, 0.0).clamp(0.0, 86_400.0);
        for step in &mut self.steps {
            step.delay_after = finite_or(step.delay_after, 0.0).clamp(0.0, 86_400.0);
        }
        self
    }

    pub fn total_passes(&self) -> Option<u64> {
        if self.loop_forever {
            None
        } else {
            Some(self.repeat_count.max(1))
        }
    }

    pub fn estimated_pass_secs(&self) -> f64 {
        self.steps.iter().map(step_estimate_secs).sum()
    }

    pub fn estimated_total_secs(&self) -> Option<f64> {
        let passes = self.total_passes()?;
        let pass_time = self.estimated_pass_secs();
        Some(
            self.start_delay
                + pass_time * passes as f64
                + self.loop_delay * passes.saturating_sub(1) as f64,
        )
    }
}

fn step_estimate_secs(step: &MacroStep) -> f64 {
    let action_cost = match &step.action {
        MacroAction::Click { .. } => 0.02,
        MacroAction::Move { .. } => 0.01,
        MacroAction::Pause => 0.0,
        MacroAction::Text { text } => (text.chars().count() as f64 * 0.012).min(2.0),
        MacroAction::KeyPress { .. } => 0.02,
        MacroAction::Scroll { .. } => 0.01,
    };
    step.delay_after + action_cost
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SavedConfig {
    pub active_tab: TabKind,
    pub clicker: ClickerConfig,
    pub macro_flow: MacroConfig,
}

pub fn config_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("ClickFlow")
            .join("settings.ron")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home_dir)
            .join("ClickFlow")
            .join("settings.ron")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config"))
            .join("clickflow")
            .join("settings.ron")
    }
}

pub fn load_saved_config() -> (SavedConfig, Option<String>) {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(contents) => match ron::from_str::<SavedConfig>(&contents) {
            Ok(config) => (config, None),
            Err(error) => (
                SavedConfig::default(),
                Some(format!(
                    "Could not read {}: {error}. Using defaults.",
                    path.display()
                )),
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (SavedConfig::default(), None)
        }
        Err(error) => (
            SavedConfig::default(),
            Some(format!(
                "Could not open {}: {error}. Using defaults.",
                path.display()
            )),
        ),
    }
}

pub fn save_config(config: &SavedConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    let contents = ron::ser::to_string_pretty(config, ron::ser::PrettyConfig::default())
        .map_err(|error| format!("Could not serialize settings: {error}"))?;
    fs::write(&path, contents)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))
}

pub fn human_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "—".to_owned();
    }
    if seconds <= 0.0 {
        return "0 ms".to_owned();
    }
    if seconds < 1.0 {
        return format!("{:.0} ms", (seconds * 1000.0).round().max(0.0));
    }
    if seconds < 10.0 {
        return format!("{seconds:.2} s");
    }
    if seconds < 60.0 {
        return format!("{seconds:.1} s");
    }
    if seconds < 3600.0 {
        let minutes = (seconds / 60.0).floor() as u64;
        let rest = (seconds - minutes as f64 * 60.0).round() as u64;
        return if rest == 0 {
            format!("{minutes} min")
        } else {
            format!("{minutes} min {rest} s")
        };
    }
    let hours = (seconds / 3600.0).floor() as u64;
    let minutes = ((seconds - hours as f64 * 3600.0) / 60.0).round() as u64;
    if minutes == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {minutes} min")
    }
}

pub fn format_clock(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_can_cap_the_click_rate() {
        let config = ClickerConfig {
            rate: 100.0,
            before_click: 0.02,
            hold_time: 0.01,
            after_click: 0.02,
            ..Default::default()
        };
        assert!((config.base_interval_secs() - 0.01).abs() < 1e-9);
        assert!((config.effective_interval_secs() - 0.05).abs() < 1e-9);
        assert!((config.effective_cps() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn per_minute_interval_is_converted() {
        let config = ClickerConfig {
            rate: 120.0,
            rate_unit: RateUnit::PerMinute,
            ..Default::default()
        };
        assert!((config.base_interval_secs() - 0.5).abs() < 1e-9);
        assert!((config.effective_cps() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn infinite_loops_have_no_total() {
        let config = ClickerConfig::default();
        assert_eq!(config.total_clicks(), None);
    }

    #[test]
    fn macro_passes_are_counted() {
        let config = MacroConfig {
            loop_forever: false,
            repeat_count: 4,
            steps: vec![MacroStep::new(ActionKind::Pause)],
            ..Default::default()
        };
        assert_eq!(config.total_passes(), Some(4));
        assert!(config.estimated_total_secs().unwrap() > 4.0);
    }

    #[test]
    fn saved_config_round_trips_through_ron() {
        let mut config = SavedConfig {
            active_tab: TabKind::Macro,
            ..Default::default()
        };
        config
            .macro_flow
            .steps
            .push(MacroStep::new(ActionKind::Text));
        let text = ron::ser::to_string_pretty(&config, ron::ser::PrettyConfig::default())
            .expect("serialize");
        let decoded: SavedConfig = ron::from_str(&text).expect("deserialize");
        assert_eq!(config, decoded);
    }

    #[test]
    fn durations_are_human_readable() {
        assert_eq!(human_duration(0.1), "100 ms");
        assert_eq!(human_duration(1.5), "1.50 s");
        assert_eq!(human_duration(75.0), "1 min 15 s");
    }
}
