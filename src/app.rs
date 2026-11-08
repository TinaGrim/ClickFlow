use crate::engine::{self, EngineEvent, EngineHandle, RunConfig};
use crate::model::{
    self, ActionKind, ClickerConfig, MacroAction, MacroConfig, MacroStep, MouseButtonChoice,
    RateUnit, SavedConfig, ScrollDirection, TabKind, TargetMode,
};
use crate::platform;
use crate::theme::{
    self, ACCENT, ACCENT_DIM, BG, BORDER, BORDER_SOFT, DANGER, DANGER_BG, DIM, FIELD, INK, MUTED,
    PANEL, SUCCESS, SUCCESS_DIM, SURFACE, TEXT, WARN_BG,
};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Rect,
    ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2,
};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey},
};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

const AUTOSAVE_DELAY: Duration = Duration::from_millis(800);
const AUTOSAVE_RETRY: Duration = Duration::from_secs(5);
const NOTICE_LIFETIME: Duration = Duration::from_secs(12);

pub struct AutoClickerApp {
    active_tab: TabKind,
    clicker: ClickerConfig,
    macro_flow: MacroConfig,
    engine: Option<EngineHandle>,
    run: RunState,
    hotkeys: Option<HotkeyBinding>,
    hotkeys_attempted: bool,
    hotkey_error: Option<String>,
    accessibility_ok: bool,
    notice: Option<Notice>,
    save_error: Option<String>,
    last_saved: SavedConfig,
    last_save_attempt: Instant,
}

impl AutoClickerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let (saved, load_error) = model::load_saved_config();
        let notice = load_error.map(Notice::error);
        let accessibility_ok = platform::accessibility_trusted();
        let last_saved = saved.clone();

        Self {
            active_tab: saved.active_tab,
            clicker: saved.clicker,
            macro_flow: saved.macro_flow,
            engine: None,
            run: RunState::default(),
            hotkeys: None,
            hotkeys_attempted: false,
            hotkey_error: None,
            accessibility_ok,
            notice,
            save_error: None,
            last_saved,
            last_save_attempt: Instant::now(),
        }
    }

    fn saved_config(&self) -> SavedConfig {
        SavedConfig {
            active_tab: self.active_tab,
            clicker: self.clicker.clone(),
            macro_flow: self.macro_flow.clone(),
        }
    }

    fn autosave(&mut self) {
        let current = self.saved_config();
        if current == self.last_saved {
            return;
        }

        let wait = if self.save_error.is_some() {
            AUTOSAVE_RETRY
        } else {
            AUTOSAVE_DELAY
        };
        if self.last_save_attempt.elapsed() < wait {
            return;
        }
        self.last_save_attempt = Instant::now();

        match model::save_config(&current) {
            Ok(()) => {
                self.last_saved = current;
                self.save_error = None;
            }
            Err(error) => {
                let is_new = self.save_error.as_deref() != Some(error.as_str());
                self.save_error = Some(error.clone());
                if is_new {
                    self.notice = Some(Notice::error(error));
                }
            }
        }
    }

    fn init_hotkeys(&mut self) {
        if self.hotkeys_attempted {
            return;
        }
        self.hotkeys_attempted = true;

        let binding = (|| -> Result<HotkeyBinding, String> {
            let manager = GlobalHotKeyManager::new()
                .map_err(|error| format!("Global shortcuts unavailable: {error}"))?;
            let start = HotKey::new(None, Code::F6);
            let stop = HotKey::new(None, Code::F7);
            manager
                .register(start)
                .map_err(|error| format!("F6 could not be registered: {error}"))?;
            manager
                .register(stop)
                .map_err(|error| format!("F7 could not be registered: {error}"))?;
            Ok(HotkeyBinding {
                _manager: manager,
                start_id: start.id(),
                stop_id: stop.id(),
            })
        })();

        match binding {
            Ok(binding) => {
                self.hotkeys = Some(binding);
                self.hotkey_error = None;
            }
            Err(error) => self.hotkey_error = Some(error),
        }
    }

    fn poll_hotkeys(&self) -> RequestedAction {
        let mut action = RequestedAction::default();
        let Some(binding) = &self.hotkeys else {
            return action;
        };

        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state() != HotKeyState::Pressed {
                continue;
            }
            if event.id() == binding.start_id {
                action.start = true;
            } else if event.id() == binding.stop_id {
                action.stop = true;
            }
        }
        action
    }

    fn poll_engine(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;

        if let Some(engine) = &self.engine {
            loop {
                match engine.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        let mut terminal = false;
        for event in events {
            if self.apply_engine_event(event) {
                terminal = true;
                break;
            }
        }

        if terminal {
            self.engine = None;
        } else if disconnected && self.engine.is_some() {
            self.engine = None;
            if !matches!(
                self.run.phase,
                RunPhase::Finished | RunPhase::Stopped | RunPhase::Error
            ) {
                self.run.phase = RunPhase::Error;
                self.run.ended_at = Some(Instant::now());
                self.run.error = Some(
                    "The click engine closed unexpectedly. Start a new run to try again.".into(),
                );
            }
        }
    }

    fn apply_engine_event(&mut self, event: EngineEvent) -> bool {
        match event {
            EngineEvent::Countdown { remaining_secs } => {
                if self.run.phase == RunPhase::Starting {
                    self.run.countdown = remaining_secs.max(0.0);
                }
                false
            }
            EngineEvent::Started { total } => {
                if self.run.phase == RunPhase::Starting {
                    self.run.phase = RunPhase::Running;
                }
                self.run.total = total;
                self.run.countdown = 0.0;
                false
            }
            EngineEvent::Progress { completed, total } => {
                self.run.completed = completed;
                self.run.total = total;
                self.run.last_activity = Some(Instant::now());
                false
            }
            EngineEvent::Finished {
                completed,
                total,
                stopped,
                error,
            } => {
                self.run.completed = completed;
                self.run.total = total;
                self.run.ended_at = Some(Instant::now());
                self.run.countdown = 0.0;
                if let Some(error) = error {
                    self.run.phase = RunPhase::Error;
                    self.run.error = Some(error);
                } else if stopped {
                    self.run.phase = RunPhase::Stopped;
                } else {
                    self.run.phase = RunPhase::Finished;
                }
                true
            }
        }
    }

    fn start_run(&mut self) {
        if self.engine.is_some() {
            return;
        }

        let mode = self.active_tab;
        let config = match mode {
            TabKind::Clicker => RunConfig::Clicker(self.clicker.clone().sanitized()),
            TabKind::Macro => {
                if self.macro_flow.steps.is_empty() {
                    self.notice = Some(Notice::error(
                        "Add at least one step before starting the macro.",
                    ));
                    return;
                }
                RunConfig::Macro(self.macro_flow.clone().sanitized())
            }
        };

        let total = config.total();
        let delay = config.start_delay();
        let snapshot_cps = self.clicker.clone().sanitized().effective_cps();
        let snapshot_rate_value = self.clicker.rate;
        let snapshot_rate_unit = self.clicker.rate_unit;
        let snapshot_steps = self.macro_flow.steps.len();

        match EngineHandle::spawn(config) {
            Ok(engine) => {
                self.engine = Some(engine);
                self.run = RunState {
                    phase: RunPhase::Starting,
                    mode,
                    total,
                    countdown: delay.max(0.0),
                    started_at: Some(Instant::now()),
                    snapshot_cps,
                    snapshot_rate_value,
                    snapshot_rate_unit,
                    snapshot_steps,
                    ..RunState::default()
                };
            }
            Err(error) => {
                self.run = RunState {
                    phase: RunPhase::Error,
                    mode,
                    error: Some(error),
                    ..RunState::default()
                };
            }
        }
    }

    fn stop_run(&mut self) {
        if let Some(engine) = &self.engine
            && matches!(self.run.phase, RunPhase::Starting | RunPhase::Running)
        {
            engine.request_stop();
            self.run.phase = RunPhase::Stopping;
        }
    }

    fn header_ui(&mut self, ctx: &egui::Context) {
        let dirty = self.saved_config() != self.last_saved;

        egui::TopBottomPanel::top("header")
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(24, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    logo_mark(ui);
                    ui.add_space(2.0);
                    ui.label(theme::semibold("ClickFlow", 17.0).color(TEXT));
                    ui.add_space(16.0);

                    if tab_button(ui, self.active_tab == TabKind::Clicker, "Clicker").clicked() {
                        self.active_tab = TabKind::Clicker;
                    }
                    if tab_button(ui, self.active_tab == TabKind::Macro, "Macro").clicked() {
                        self.active_tab = TabKind::Macro;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(error) = &self.save_error {
                            ui.label(theme::body("Not saved", 10.5).color(DANGER))
                                .on_hover_text(error);
                        } else if dirty {
                            ui.label(theme::muted("Saving…", 10.5));
                        } else {
                            ui.label(theme::muted("Saved", 10.5));
                        }
                    });
                });

                let rect = ui.max_rect();
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() + 13.0,
                    Stroke::new(1.0_f32, BORDER_SOFT),
                );
            });
    }

    fn sidebar_ui(&mut self, ctx: &egui::Context) -> RequestedAction {
        let mut action = RequestedAction::default();

        egui::SidePanel::right("run_panel")
            .resizable(false)
            .exact_width(330.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(22, 20)),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(theme::semibold("Run", 16.0));
                        ui.add_space(14.0);

                        self.status_ui(ui);
                        ui.add_space(18.0);
                        self.readout_ui(ui);
                        ui.add_space(18.0);

                        let active = self.engine.is_some();
                        let stopping = self.run.phase == RunPhase::Stopping;
                        let label = if stopping {
                            "Stopping…"
                        } else if active {
                            "Stop run"
                        } else {
                            match self.active_tab {
                                TabKind::Clicker => "Start clicker",
                                TabKind::Macro => "Start macro",
                            }
                        };

                        let (fill, text_color, border) = if stopping {
                            (FIELD, DIM, BORDER)
                        } else if active {
                            (DANGER_BG, DANGER, DANGER)
                        } else {
                            (ACCENT, INK, ACCENT)
                        };

                        let button =
                            egui::Button::new(theme::semibold(label, 14.0).color(text_color))
                                .fill(fill)
                                .stroke(Stroke::new(1.0_f32, border))
                                .corner_radius(CornerRadius::same(8));

                        let clicked = if stopping {
                            ui.add_enabled(
                                false,
                                egui::Button::new(theme::semibold(label, 14.0).color(DIM))
                                    .fill(FIELD)
                                    .stroke(Stroke::new(1.0_f32, BORDER))
                                    .corner_radius(CornerRadius::same(8)),
                            )
                            .clicked()
                        } else {
                            ui.add_sized([ui.available_width(), 44.0], button)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                        };

                        if clicked {
                            if active {
                                action.stop = true;
                            } else {
                                action.start = true;
                            }
                        }

                        ui.add_space(10.0);
                        self.shortcut_ui(ui);
                        ui.add_space(18.0);

                        theme::hairline(ui);
                        self.progress_ui(ui);

                        ui.add_space(18.0);
                        theme::hairline(ui);
                        ui.label(theme::muted("Settings save automatically.", 10.5));
                        if cfg!(target_os = "macos") {
                            ui.add_space(5.0);
                            let (color, label) = if self.accessibility_ok {
                                (SUCCESS, "Accessibility allowed")
                            } else {
                                (DANGER, "Accessibility needed")
                            };
                            ui.horizontal(|ui| {
                                status_dot(ui, color, None, 12.0);
                                ui.label(theme::muted(label, 10.5));
                            });
                        }
                        ui.add_space(8.0);
                        ui.label(theme::muted("ClickFlow 0.1", 9.5).color(DIM));
                    });
            });

        action
    }

    fn status_ui(&self, ui: &mut Ui) {
        let (color, text) = match self.run.phase {
            RunPhase::Idle => (MUTED, "Ready".to_owned()),
            RunPhase::Starting => (
                ACCENT,
                format!("Starting in {:.1} s", self.run.countdown.max(0.0)),
            ),
            RunPhase::Running => (
                SUCCESS,
                match self.run.mode {
                    TabKind::Clicker => "Clicking".to_owned(),
                    TabKind::Macro => "Playing macro".to_owned(),
                },
            ),
            RunPhase::Stopping => (MUTED, "Stopping".to_owned()),
            RunPhase::Stopped => (MUTED, "Stopped".to_owned()),
            RunPhase::Finished => (SUCCESS, "Finished".to_owned()),
            RunPhase::Error => (DANGER, "Needs attention".to_owned()),
        };

        ui.horizontal(|ui| {
            let pulse = if matches!(self.run.phase, RunPhase::Running) {
                self.run.last_activity
            } else {
                None
            };
            status_dot(ui, color, pulse, 16.0);
            ui.label(theme::semibold(text, 13.0).color(color));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.run.started_at.is_some() {
                    ui.label(
                        theme::mono(model::format_clock(self.run.elapsed_secs()), 11.0).color(DIM),
                    );
                }
            });
        });
    }

    fn readout_ui(&self, ui: &mut Ui) {
        let active = self.engine.is_some();
        let mode = if active {
            self.run.mode
        } else {
            self.active_tab
        };

        match mode {
            TabKind::Clicker => {
                let effective_cps = if active {
                    self.run.snapshot_cps
                } else {
                    self.clicker.effective_cps()
                };
                let unit = if active {
                    self.run.snapshot_rate_unit
                } else {
                    self.clicker.rate_unit
                };
                let value = match unit {
                    RateUnit::PerSecond => effective_cps,
                    RateUnit::PerMinute => effective_cps * 60.0,
                };
                let configured_value = if active {
                    self.run.snapshot_rate_value
                } else {
                    self.clicker.rate
                };
                let interval = if effective_cps > 0.0 {
                    1.0 / effective_cps
                } else {
                    f64::INFINITY
                };

                ui.label(theme::muted("Effective speed", 10.5));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(
                        theme::mono(grouped_number(value, metric_decimals(value)), 44.0)
                            .color(TEXT),
                    );
                    ui.add_space(2.0);
                    ui.label(theme::muted(
                        match unit {
                            RateUnit::PerSecond => "clicks / second",
                            RateUnit::PerMinute => "clicks / minute",
                        },
                        10.5,
                    ));
                });
                ui.add_space(4.0);
                draw_rate_meter(ui, effective_cps);
                ui.add_space(2.0);
                ui.label(theme::muted(
                    format!(
                        "Every {} · configured {} per {}",
                        model::human_duration(interval),
                        trim_number(configured_value, 2),
                        unit.short_label()
                    ),
                    10.5,
                ));
            }
            TabKind::Macro => {
                let steps = if active {
                    self.run.snapshot_steps
                } else {
                    self.macro_flow.steps.len()
                };
                let passes = if active {
                    self.run.total
                } else {
                    self.macro_flow.total_passes()
                };
                let pass_time = self.macro_flow.estimated_pass_secs();
                let total_time = self.macro_flow.estimated_total_secs();

                ui.label(theme::muted("Macro sequence", 10.5));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(theme::mono(grouped_number(steps as f64, 0), 44.0).color(TEXT));
                    ui.add_space(2.0);
                    ui.label(theme::muted(
                        if steps == 1 { "step" } else { "steps" },
                        10.5,
                    ));
                });
                ui.add_space(7.0);
                let passes_text = match (passes, total_time) {
                    (Some(count), Some(total_time)) => format!(
                        "{} {} · about {} total",
                        count,
                        if count == 1 { "pass" } else { "passes" },
                        model::human_duration(total_time)
                    ),
                    (None, _) => format!(
                        "Loops until stopped · about {} per pass",
                        model::human_duration(pass_time)
                    ),
                    (Some(count), None) => {
                        format!("{} {}", count, if count == 1 { "pass" } else { "passes" })
                    }
                };
                ui.label(theme::muted(passes_text, 10.5));
                if steps == 0 {
                    ui.add_space(6.0);
                    ui.label(theme::body("Add steps below to build a run.", 11.0).color(ACCENT));
                }
            }
        }
    }

    fn shortcut_ui(&self, ui: &mut Ui) {
        if let Some(error) = &self.hotkey_error {
            ui.label(theme::body("Global shortcuts unavailable", 10.5).color(DANGER))
                .on_hover_text(error);
            return;
        }

        ui.horizontal(|ui| {
            keycap(ui, "F6");
            ui.label(theme::muted("start", 10.5));
            ui.add_space(6.0);
            keycap(ui, "F7");
            ui.label(theme::muted("stop", 10.5));
        });
    }

    fn progress_ui(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(theme::muted("Progress", 10.5));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.run.started_at.is_some() {
                    ui.label(
                        theme::mono(model::format_clock(self.run.elapsed_secs()), 10.5).color(DIM),
                    );
                }
            });
        });
        ui.add_space(5.0);

        if self.run.started_at.is_none() {
            ui.label(theme::body("Not started yet.", 12.0).color(MUTED));
            ui.add_space(7.0);
            progress_rail(ui, Some(0.0), ACCENT, false);
            return;
        }

        let unit = match self.run.mode {
            TabKind::Clicker => {
                if self.run.completed == 1 {
                    "click"
                } else {
                    "clicks"
                }
            }
            TabKind::Macro => {
                if self.run.completed == 1 {
                    "pass"
                } else {
                    "passes"
                }
            }
        };

        match self.run.total {
            Some(total) => {
                ui.horizontal(|ui| {
                    ui.label(
                        theme::mono(format!("{} / {}", self.run.completed, total), 16.0)
                            .color(TEXT),
                    );
                    ui.label(theme::muted(unit, 10.5));
                });
                ui.add_space(7.0);
                let fraction = if total == 0 {
                    0.0
                } else {
                    (self.run.completed as f32 / total as f32).clamp(0.0, 1.0)
                };
                let color = match self.run.phase {
                    RunPhase::Finished => SUCCESS,
                    RunPhase::Error => DANGER,
                    _ => ACCENT,
                };
                progress_rail(ui, Some(fraction), color, false);
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label(theme::mono(self.run.completed.to_string(), 16.0).color(TEXT));
                    ui.label(theme::muted(unit, 10.5));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(theme::muted("No limit", 10.0).color(ACCENT));
                    });
                });
                ui.add_space(7.0);
                progress_rail(ui, None, ACCENT, self.engine.is_some());
            }
        }

        if self.run.phase == RunPhase::Error
            && let Some(error) = &self.run.error
        {
            ui.add_space(9.0);
            ui.label(theme::body(error, 10.5).color(DANGER));
        }
    }

    fn central_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BG)
                    .inner_margin(Margin::symmetric(24, 20)),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(900.0);
                        self.page_intro_ui(ui);

                        if self.engine.is_some() {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::new(6.0, 6.0), Sense::hover());
                                ui.painter().circle_filled(rect.center(), 3.0, ACCENT);
                                ui.label(theme::muted(
                                    "A run is active. Settings changes apply to the next run.",
                                    10.5,
                                ));
                            });
                        }

                        if cfg!(target_os = "macos") && !self.accessibility_ok {
                            ui.add_space(12.0);
                            self.accessibility_banner_ui(ui);
                        }

                        if let Some(error) = &self.run.error {
                            ui.add_space(12.0);
                            danger_banner(ui, "The run stopped early", error);
                        }

                        if let Some(notice) = &self.notice
                            && notice.at.elapsed() < NOTICE_LIFETIME
                        {
                            ui.add_space(12.0);
                            notice_banner(ui, notice);
                        }

                        ui.add_space(20.0);
                        match self.active_tab {
                            TabKind::Clicker => self.clicker_settings_ui(ui),
                            TabKind::Macro => self.macro_settings_ui(ui),
                        }

                        ui.add_space(24.0);
                        theme::hairline(ui);
                        ui.label(theme::muted("ClickFlow 0.1", 9.5).color(DIM));
                        ui.add_space(10.0);
                    });
            });
    }

    fn page_intro_ui(&self, ui: &mut Ui) {
        let (title, subtitle) = match self.active_tab {
            TabKind::Clicker => (
                "Click settings",
                "Set a rhythm, choose where clicks land, and give yourself time to switch apps.",
            ),
            TabKind::Macro => (
                "Macro editor",
                "Build a sequence of clicks, movement, pauses, text, keys, and scrolling.",
            ),
        };
        ui.add_space(2.0);
        ui.label(theme::semibold(title, 22.0).color(TEXT));
        ui.add_space(3.0);
        ui.label(theme::muted(subtitle, 11.5));
    }

    fn accessibility_banner_ui(&mut self, ui: &mut Ui) {
        Frame::new()
            .fill(WARN_BG)
            .stroke(Stroke::new(1.0_f32, ACCENT_DIM))
            .corner_radius(CornerRadius::same(9))
            .inner_margin(Margin::symmetric(16, 13))
            .show(ui, |ui| {
                ui.label(theme::semibold("macOS needs accessibility access", 12.5));
                ui.add_space(3.0);
                ui.label(theme::muted(
                    "macOS blocks synthetic clicks until access is granted. When running from source, enable your terminal app.",
                    11.0,
                ));
                ui.add_space(9.0);
                ui.horizontal(|ui| {
                    if ui.button("Open System Settings").clicked()
                        && let Err(error) = platform::open_accessibility_settings()
                    {
                        self.notice = Some(Notice::error(error));
                    }
                    if ui.button("Check again").clicked() {
                        self.accessibility_ok = platform::accessibility_trusted();
                        if !self.accessibility_ok {
                            self.notice = Some(Notice::error(
                                "Accessibility access is still off. Enable the app, then check again.",
                            ));
                        }
                    }
                });
            });
    }

    fn clicker_settings_ui(&mut self, ui: &mut Ui) {
        section_header(
            ui,
            "Speed",
            "Choose a cadence. The pause controls below can cap the top speed.",
        );
        self.speed_ui(ui);

        theme::hairline(ui);
        section_header(
            ui,
            "Timing",
            "Short pauses can make a run easier to control and less mechanical.",
        );
        self.timing_ui(ui);

        theme::hairline(ui);
        section_header(
            ui,
            "Target",
            "Click wherever the pointer is, or lock every click to one point.",
        );
        self.target_ui(ui);

        theme::hairline(ui);
        section_header(
            ui,
            "Repeat",
            "Run indefinitely, or stop automatically after a set number of clicks.",
        );
        self.repeat_ui(ui);
    }

    fn speed_ui(&mut self, ui: &mut Ui) {
        let old_unit = self.clicker.rate_unit;
        let mut chosen_unit = old_unit;

        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Body,
                    FontId::new(21.0, FontFamily::Monospace),
                );
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Button,
                    FontId::new(21.0, FontFamily::Monospace),
                );
                let range = match chosen_unit {
                    RateUnit::PerSecond => 0.01..=1_000.0,
                    RateUnit::PerMinute => 1.0..=60_000.0,
                };
                let speed = match chosen_unit {
                    RateUnit::PerSecond => 0.1,
                    RateUnit::PerMinute => 1.0,
                };
                ui.add_sized(
                    [120.0, 38.0],
                    egui::DragValue::new(&mut self.clicker.rate)
                        .speed(speed)
                        .range(range)
                        .max_decimals(2),
                );
            });
            ui.label(theme::muted("clicks", 11.5));
            ui.add_space(12.0);
            ui.label(theme::muted("per", 11.5));
            segmented(
                ui,
                &mut chosen_unit,
                &[
                    (RateUnit::PerSecond, "second"),
                    (RateUnit::PerMinute, "minute"),
                ],
            );
        });

        if chosen_unit != old_unit {
            self.clicker.rate = convert_rate(self.clicker.rate, old_unit, chosen_unit);
            self.clicker.rate_unit = chosen_unit;
        }

        let effective = self.clicker.clone().sanitized().effective_interval_secs();
        let base = self.clicker.base_interval_secs();
        let limited = effective > base * 1.001;

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(theme::muted("Effective", 10.5));
            let effective_cps = 1.0 / effective;
            ui.label(
                theme::mono(
                    match self.clicker.rate_unit {
                        RateUnit::PerSecond => {
                            format!("{} clicks/sec", trim_number(effective_cps, 2))
                        }
                        RateUnit::PerMinute => {
                            format!("{} clicks/min", trim_number(effective_cps * 60.0, 1))
                        }
                    },
                    11.5,
                )
                .color(TEXT),
            );
            ui.label(theme::muted("·", 10.0).color(DIM));
            ui.label(theme::muted(
                format!("every {}", model::human_duration(effective)),
                10.5,
            ));
        });

        if limited {
            ui.add_space(5.0);
            ui.label(
                theme::body("The pauses below are limiting the actual click rate.", 10.5)
                    .color(ACCENT),
            );
        }
    }

    fn timing_ui(&mut self, ui: &mut Ui) {
        ui.columns(4, |columns| {
            seconds_field(
                &mut columns[0],
                "Start delay",
                &mut self.clicker.start_delay,
                0.0..=3600.0,
                "Time to switch to the target app before the first click.",
            );
            seconds_field(
                &mut columns[1],
                "Before press",
                &mut self.clicker.before_click,
                0.0..=3600.0,
                "A pause immediately before each mouse press.",
            );
            seconds_field(
                &mut columns[2],
                "Button hold",
                &mut self.clicker.hold_time,
                0.0..=2.0,
                "How long the mouse button stays pressed.",
            );
            seconds_field(
                &mut columns[3],
                "After release",
                &mut self.clicker.after_click,
                0.0..=3600.0,
                "A pause immediately after each mouse release.",
            );
        });
    }

    fn target_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(theme::muted("Target", 11.0));
            segmented(
                ui,
                &mut self.clicker.target,
                &[
                    (TargetMode::Cursor, "Current cursor"),
                    (TargetMode::Fixed, "Fixed point"),
                ],
            );
            ui.add_space(12.0);
            ui.label(theme::muted("Button", 11.0));
            segmented(
                ui,
                &mut self.clicker.button,
                &[
                    (MouseButtonChoice::Left, "Left"),
                    (MouseButtonChoice::Right, "Right"),
                    (MouseButtonChoice::Middle, "Middle"),
                ],
            );
        });

        ui.add_space(9.0);
        match self.clicker.target {
            TargetMode::Cursor => {
                ui.label(theme::muted(
                    "Each click lands wherever the pointer is at that moment.",
                    10.5,
                ));
            }
            TargetMode::Fixed => {
                ui.horizontal(|ui| {
                    ui.label(theme::muted("x", 10.5));
                    ui.add_sized(
                        [72.0, 29.0],
                        egui::DragValue::new(&mut self.clicker.x)
                            .speed(1.0)
                            .range(-100_000..=100_000),
                    );
                    ui.label(theme::muted("y", 10.5));
                    ui.add_sized(
                        [72.0, 29.0],
                        egui::DragValue::new(&mut self.clicker.y)
                            .speed(1.0)
                            .range(-100_000..=100_000),
                    );
                    if ui.button("Use current cursor").clicked() {
                        match engine::cursor_position() {
                            Ok((x, y)) => {
                                self.clicker.x = x;
                                self.clicker.y = y;
                                self.notice = Some(Notice::success("Cursor position captured."));
                            }
                            Err(error) => {
                                self.notice = Some(Notice::error(error));
                            }
                        }
                    }
                });
                ui.add_space(5.0);
                ui.label(theme::muted(
                    "Every click is sent to this exact screen coordinate.",
                    10.5,
                ));
            }
        }
    }

    fn repeat_ui(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.clicker.loop_forever, "Loop until stopped");

        if self.clicker.loop_forever {
            ui.add_space(5.0);
            ui.label(theme::muted(
                "Runs until you press F7 or use the Stop button.",
                10.5,
            ));
        } else {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(theme::muted("Stop after", 11.0));
                ui.add(
                    egui::DragValue::new(&mut self.clicker.repeat_count)
                        .speed(1.0)
                        .range(1..=100_000_000_u64),
                );
                ui.label(theme::muted("clicks", 11.0));
            });
            if let Some(total) = self.clicker.estimated_total_secs() {
                ui.add_space(5.0);
                ui.label(theme::muted(
                    format!(
                        "About {} at the effective rate.",
                        model::human_duration(total)
                    ),
                    10.5,
                ));
            }
        }
    }

    fn macro_settings_ui(&mut self, ui: &mut Ui) {
        section_header(
            ui,
            "Timing and repeats",
            "Each step has its own pause. This sets the overall start and loop timing.",
        );

        ui.columns(3, |columns| {
            seconds_field(
                &mut columns[0],
                "Start delay",
                &mut self.macro_flow.start_delay,
                0.0..=3600.0,
                "Time to switch to the target app before the macro begins.",
            );
            seconds_field(
                &mut columns[1],
                "Between passes",
                &mut self.macro_flow.loop_delay,
                0.0..=3600.0,
                "A pause after a complete pass, before the next pass starts.",
            );

            let loop_ui = &mut columns[2];
            loop_ui.label(theme::muted("Repeat", 10.5));
            loop_ui.checkbox(&mut self.macro_flow.loop_forever, "Loop until stopped");
            if !self.macro_flow.loop_forever {
                loop_ui.horizontal(|ui| {
                    ui.label(theme::muted("Passes", 10.5));
                    ui.add(
                        egui::DragValue::new(&mut self.macro_flow.repeat_count)
                            .speed(1.0)
                            .range(1..=10_000_000_u64),
                    );
                });
            }
        });

        ui.add_space(9.0);
        let steps = self.macro_flow.steps.len();
        let passes = self.macro_flow.total_passes();
        let pass_time = self.macro_flow.estimated_pass_secs();
        let summary = match passes {
            Some(count) => format!(
                "{steps} {} · {count} {} · about {} per pass",
                if steps == 1 { "step" } else { "steps" },
                if count == 1 { "pass" } else { "passes" },
                model::human_duration(pass_time)
            ),
            None => format!(
                "{steps} {} · loops until stopped · about {} per pass",
                if steps == 1 { "step" } else { "steps" },
                model::human_duration(pass_time)
            ),
        };
        ui.label(theme::muted(summary, 10.5));

        theme::hairline(ui);
        section_header(
            ui,
            "Sequence",
            "Steps run from top to bottom, then the pass repeats.",
        );

        let mut pending: Option<StepCommand> = None;

        ui.horizontal_wrapped(|ui| {
            for kind in ActionKind::ALL {
                if ui
                    .add(egui::Button::new(theme::body(kind.add_label(), 11.5)).small())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    pending = Some(StepCommand::Add(kind));
                }
            }
        });

        ui.add_space(10.0);
        Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(14, 6))
            .show(ui, |ui| {
                if self.macro_flow.steps.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(theme::semibold("No steps yet", 13.0));
                        ui.add_space(3.0);
                        ui.label(theme::muted(
                            "Add a click, movement, pause, text, key, or scroll step above.",
                            11.0,
                        ));
                    });
                    ui.add_space(20.0);
                    return;
                }

                let step_count = self.macro_flow.steps.len();
                for (index, step) in self.macro_flow.steps.iter_mut().enumerate() {
                    if index > 0 {
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 1.0),
                            Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 0.0, BORDER_SOFT);
                    }
                    ui.add_space(9.0);

                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(26.0, 28.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                ui.label(theme::mono(format!("{:02}", index + 1), 10.5).color(DIM));
                            },
                        );

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let old_kind = step.action.kind();
                                macro_action_picker(ui, &mut step.action, index);
                                if step.action.kind() != old_kind {
                                    step.delay_after =
                                        MacroAction::default_delay_after(step.action.kind());
                                }
                                macro_action_fields(ui, &mut step.action, index);
                            });

                            ui.add_space(1.0);
                            ui.horizontal(|ui| {
                                let remove = ui
                                    .add(
                                        egui::Button::new(
                                            theme::body("Remove", 10.0).color(DANGER),
                                        )
                                        .small(),
                                    )
                                    .on_hover_text("Remove this step")
                                    .clicked();
                                ui.add_space(4.0);

                                let label = if matches!(&step.action, MacroAction::Pause) {
                                    "Wait"
                                } else {
                                    "Delay after"
                                };
                                ui.label(theme::muted(label, 10.5));
                                ui.add(
                                    egui::DragValue::new(&mut step.delay_after)
                                        .speed(0.02)
                                        .range(0.0..=3600.0)
                                        .max_decimals(2)
                                        .suffix(" s"),
                                );

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let down = ui
                                        .add_enabled(
                                            index + 1 < step_count,
                                            egui::Button::new("↓").small(),
                                        )
                                        .on_hover_text("Move down")
                                        .clicked();
                                    let up = ui
                                        .add_enabled(index > 0, egui::Button::new("↑").small())
                                        .on_hover_text("Move up")
                                        .clicked();
                                    if remove {
                                        pending = Some(StepCommand::Remove(index));
                                    } else if down {
                                        pending = Some(StepCommand::MoveDown(index));
                                    } else if up {
                                        pending = Some(StepCommand::MoveUp(index));
                                    }
                                });
                            });
                        });
                    });
                    ui.add_space(9.0);
                }
            });

        match pending {
            Some(StepCommand::Add(kind)) => self.macro_flow.steps.push(MacroStep::new(kind)),
            Some(StepCommand::MoveUp(index)) if index > 0 => {
                self.macro_flow.steps.swap(index, index - 1);
            }
            Some(StepCommand::MoveDown(index)) if index + 1 < self.macro_flow.steps.len() => {
                self.macro_flow.steps.swap(index, index + 1);
            }
            Some(StepCommand::Remove(index)) if index < self.macro_flow.steps.len() => {
                self.macro_flow.steps.remove(index);
            }
            _ => {}
        }
    }
}

impl eframe::App for AutoClickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.init_hotkeys();
        let mut action = self.poll_hotkeys();
        self.poll_engine();

        if self
            .notice
            .as_ref()
            .is_some_and(|notice| notice.at.elapsed() >= NOTICE_LIFETIME)
        {
            self.notice = None;
        }

        self.header_ui(ctx);
        let sidebar_action = self.sidebar_ui(ctx);
        action.start |= sidebar_action.start;
        action.stop |= sidebar_action.stop;

        self.central_ui(ctx);

        if action.stop {
            self.stop_run();
        } else if action.start {
            self.start_run();
        }

        self.autosave();

        let repaint = if self.engine.is_some() {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(250)
        };
        ctx.request_repaint_after(repaint);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = model::save_config(&self.saved_config());
    }
}

#[derive(Default)]
struct RunState {
    phase: RunPhase,
    mode: TabKind,
    completed: u64,
    total: Option<u64>,
    countdown: f64,
    started_at: Option<Instant>,
    ended_at: Option<Instant>,
    last_activity: Option<Instant>,
    error: Option<String>,
    snapshot_cps: f64,
    snapshot_rate_value: f64,
    snapshot_rate_unit: RateUnit,
    snapshot_steps: usize,
}

impl RunState {
    fn elapsed_secs(&self) -> f64 {
        match (self.started_at, self.ended_at) {
            (Some(start), Some(end)) => end.saturating_duration_since(start).as_secs_f64(),
            (Some(start), None) => start.elapsed().as_secs_f64(),
            _ => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RunPhase {
    #[default]
    Idle,
    Starting,
    Running,
    Stopping,
    Stopped,
    Finished,
    Error,
}

struct HotkeyBinding {
    _manager: GlobalHotKeyManager,
    start_id: u32,
    stop_id: u32,
}

struct Notice {
    message: String,
    error: bool,
    at: Instant,
}

impl Notice {
    fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            error: true,
            at: Instant::now(),
        }
    }

    fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            error: false,
            at: Instant::now(),
        }
    }
}

#[derive(Default, Clone, Copy)]
struct RequestedAction {
    start: bool,
    stop: bool,
}

enum StepCommand {
    Add(ActionKind),
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
}

fn section_header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(theme::semibold(title, 14.5).color(TEXT));
    ui.add_space(2.0);
    ui.label(theme::muted(subtitle, 10.5));
    ui.add_space(11.0);
}

fn seconds_field(
    ui: &mut Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    tooltip: &str,
) {
    ui.label(theme::muted(label, 10.5)).on_hover_text(tooltip);
    ui.add(
        egui::DragValue::new(value)
            .speed(0.01)
            .range(range)
            .max_decimals(3)
            .suffix(" s"),
    );
}

fn segmented<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    Frame::new()
        .fill(FIELD)
        .stroke(Stroke::new(1.0_f32, BORDER_SOFT))
        .corner_radius(CornerRadius::same(7))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (option, label) in options {
                    let selected = *value == *option;
                    let button = egui::Button::new(
                        theme::body(*label, if selected { 11.5 } else { 11.0 })
                            .color(if selected { INK } else { MUTED }),
                    )
                    .fill(if selected {
                        ACCENT
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(5))
                    .min_size(Vec2::new(0.0, 24.0));
                    if ui.add(button).clicked() {
                        *value = *option;
                    }
                }
            });
        });
}

fn tab_button(ui: &mut Ui, selected: bool, label: &str) -> egui::Response {
    let color = if selected { TEXT } else { MUTED };
    let response = ui
        .add(
            egui::Button::new(theme::semibold(label, 13.0).color(color))
                .frame(false)
                .fill(Color32::TRANSPARENT),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    let y = response.rect.bottom() + 4.0;
    if selected {
        ui.painter().line_segment(
            [
                egui::pos2(response.rect.left(), y),
                egui::pos2(response.rect.right(), y),
            ],
            Stroke::new(2.0_f32, ACCENT),
        );
    } else if response.hovered() {
        ui.painter().line_segment(
            [
                egui::pos2(response.rect.left(), y),
                egui::pos2(response.rect.right(), y),
            ],
            Stroke::new(1.0_f32, BORDER),
        );
    }
    response
}

fn logo_mark(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(7), ACCENT);
    painter.circle_stroke(rect.center(), 6.4, Stroke::new(1.7_f32, INK));
    painter.circle_filled(rect.center(), 2.1, INK);
    painter.line_segment(
        [
            rect.center() + Vec2::new(5.2, -5.2),
            rect.center() + Vec2::new(8.4, -8.4),
        ],
        Stroke::new(1.7_f32, INK),
    );
}

fn status_dot(ui: &mut Ui, color: Color32, pulse: Option<Instant>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let center = rect.center();
    let painter = ui.painter();
    painter.circle_filled(center, size * 0.22, color);
    if let Some(pulse) = pulse {
        let elapsed = pulse.elapsed().as_secs_f32();
        let duration = 0.55;
        if elapsed < duration {
            let progress = elapsed / duration;
            let alpha = (105.0 * (1.0 - progress)).round() as u8;
            let ring = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
            painter.circle_stroke(
                center,
                size * 0.22 + progress * 7.0,
                Stroke::new(1.2_f32, ring),
            );
        }
    }
}

fn keycap(ui: &mut Ui, label: &str) {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(theme::mono(label, 9.5).color(MUTED));
        });
}

fn progress_rail(ui: &mut Ui, fraction: Option<f32>, color: Color32, animate: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), FIELD);

    if let Some(fraction) = fraction {
        let width = rect.width() * fraction.clamp(0.0, 1.0);
        if width > 0.0 {
            let filled = Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
            painter.rect_filled(filled, CornerRadius::same(3), color);
        }
    } else if animate {
        let time = ui.input(|input| input.time) as f32;
        let phase = (time * 0.55).fract();
        let segment_width = rect.width() * 0.24;
        let x = rect.left() + (rect.width() - segment_width) * phase;
        let filled = Rect::from_min_size(
            egui::pos2(x, rect.top()),
            Vec2::new(segment_width, rect.height()),
        );
        painter.rect_filled(filled, CornerRadius::same(3), color);
    } else {
        let filled = Rect::from_min_size(rect.min, Vec2::new(rect.width() * 0.2, rect.height()));
        painter.rect_filled(filled, CornerRadius::same(3), ACCENT_DIM);
    }
}

fn draw_rate_meter(ui: &mut Ui, cps: f64) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::hover());
    let painter = ui.painter();
    let baseline_y = rect.top() + 19.0;
    let width = rect.width();
    let normalized = ((cps.max(0.1).log10() + 1.0) / 4.0).clamp(0.0, 1.0) as f32;

    painter.line_segment(
        [
            egui::pos2(rect.left(), baseline_y),
            egui::pos2(rect.right(), baseline_y),
        ],
        Stroke::new(1.0_f32, BORDER),
    );

    for tick in 0..=40 {
        let t = tick as f32 / 40.0;
        let x = rect.left() + width * t;
        let major = tick % 10 == 0;
        let active = t <= normalized;
        let tick_color = if active { ACCENT } else { BORDER };
        let height = if major { 9.0 } else { 5.0 };
        painter.line_segment(
            [
                egui::pos2(x, baseline_y - height),
                egui::pos2(x, baseline_y),
            ],
            Stroke::new(if active { 1.3_f32 } else { 1.0_f32 }, tick_color),
        );
    }

    let marker_x = rect.left() + width * normalized;
    painter.line_segment(
        [
            egui::pos2(marker_x, baseline_y - 13.0),
            egui::pos2(marker_x, baseline_y + 3.0),
        ],
        Stroke::new(1.5_f32, TEXT),
    );

    for (value, label) in [
        (0.1_f32, "0.1"),
        (1.0, "1"),
        (10.0, "10"),
        (100.0, "100"),
        (1000.0, "1000"),
    ] {
        let t = (value.log10() + 1.0) / 4.0;
        let x = (rect.left() + width * t).clamp(rect.left() + 10.0, rect.right() - 10.0);
        painter.text(
            egui::pos2(x, rect.top() + 27.0),
            Align2::CENTER_TOP,
            label,
            FontId::new(8.5, FontFamily::Monospace),
            DIM,
        );
    }
}

fn danger_banner(ui: &mut Ui, title: &str, message: &str) {
    Frame::new()
        .fill(DANGER_BG)
        .stroke(Stroke::new(1.0_f32, DANGER))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.label(theme::semibold(title, 12.5).color(DANGER));
            ui.add_space(3.0);
            ui.label(theme::body(message, 11.0).color(TEXT));
        });
}

fn notice_banner(ui: &mut Ui, notice: &Notice) {
    let (fill, border) = if notice.error {
        (DANGER_BG, DANGER)
    } else {
        (SURFACE, SUCCESS_DIM)
    };
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(6.0, 6.0), Sense::hover());
                ui.painter().circle_filled(
                    rect.center(),
                    3.0,
                    if notice.error { DANGER } else { SUCCESS },
                );
                ui.label(
                    theme::body(notice.message.as_str(), 11.0).color(if notice.error {
                        TEXT
                    } else {
                        MUTED
                    }),
                );
            });
        });
}

fn macro_action_picker(ui: &mut Ui, action: &mut MacroAction, index: usize) {
    let current = action.kind();
    let mut chosen = current;
    egui::ComboBox::from_id_salt(("macro-action", index))
        .width(92.0)
        .selected_text(theme::body(current.label(), 11.5))
        .show_ui(ui, |ui| {
            for kind in ActionKind::ALL {
                ui.selectable_value(&mut chosen, kind, kind.label());
            }
        });
    if chosen != current {
        *action = MacroAction::default_for(chosen);
    }
}

fn macro_action_fields(ui: &mut Ui, action: &mut MacroAction, index: usize) {
    match action {
        MacroAction::Click {
            target,
            x,
            y,
            button,
        } => {
            let mut chosen_target = *target;
            egui::ComboBox::from_id_salt(("macro-target", index))
                .width(102.0)
                .selected_text(theme::body(
                    match chosen_target {
                        TargetMode::Cursor => "Current cursor",
                        TargetMode::Fixed => "Fixed point",
                    },
                    11.0,
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen_target, TargetMode::Cursor, "Current cursor");
                    ui.selectable_value(&mut chosen_target, TargetMode::Fixed, "Fixed point");
                });
            *target = chosen_target;

            if *target == TargetMode::Fixed {
                ui.label(theme::muted("x", 10.5));
                ui.add_sized(
                    [58.0, 27.0],
                    egui::DragValue::new(x).speed(1.0).range(-100_000..=100_000),
                );
                ui.label(theme::muted("y", 10.5));
                ui.add_sized(
                    [58.0, 27.0],
                    egui::DragValue::new(y).speed(1.0).range(-100_000..=100_000),
                );
            }

            let mut chosen_button = *button;
            egui::ComboBox::from_id_salt(("macro-button", index))
                .width(74.0)
                .selected_text(theme::body(chosen_button.label(), 11.0))
                .show_ui(ui, |ui| {
                    for option in [
                        MouseButtonChoice::Left,
                        MouseButtonChoice::Right,
                        MouseButtonChoice::Middle,
                    ] {
                        ui.selectable_value(&mut chosen_button, option, option.label());
                    }
                });
            *button = chosen_button;
        }
        MacroAction::Move { x, y } => {
            ui.label(theme::muted("x", 10.5));
            ui.add_sized(
                [70.0, 27.0],
                egui::DragValue::new(x).speed(1.0).range(-100_000..=100_000),
            );
            ui.label(theme::muted("y", 10.5));
            ui.add_sized(
                [70.0, 27.0],
                egui::DragValue::new(y).speed(1.0).range(-100_000..=100_000),
            );
        }
        MacroAction::Pause => {}
        MacroAction::Text { text } => {
            ui.add(
                TextEdit::singleline(text)
                    .hint_text("Text to type")
                    .desired_width(ui.available_width().min(250.0)),
            );
        }
        MacroAction::KeyPress { key, modifiers } => {
            ui.add_sized(
                [44.0, 27.0],
                TextEdit::singleline(key).hint_text("key"),
            )
            .on_hover_text(
                "A single character, or enter, tab, esc, space, backspace, delete, arrows, home, end, pageup, pagedown, f1–f12.",
            );
            modifier_toggle(ui, &mut modifiers.control, "Ctrl");
            modifier_toggle(ui, &mut modifiers.shift, "Shift");
            modifier_toggle(ui, &mut modifiers.alt, "Alt");
            modifier_toggle(
                ui,
                &mut modifiers.meta,
                if cfg!(target_os = "macos") {
                    "Cmd"
                } else {
                    "Win"
                },
            );
        }
        MacroAction::Scroll { amount, direction } => {
            ui.add_sized(
                [62.0, 27.0],
                egui::DragValue::new(amount)
                    .speed(0.2)
                    .range(-1_000..=1_000),
            );
            let mut chosen_direction = *direction;
            egui::ComboBox::from_id_salt(("macro-scroll", index))
                .width(92.0)
                .selected_text(theme::body(chosen_direction.label(), 11.0))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut chosen_direction,
                        ScrollDirection::Vertical,
                        "Vertical",
                    );
                    ui.selectable_value(
                        &mut chosen_direction,
                        ScrollDirection::Horizontal,
                        "Horizontal",
                    );
                });
            *direction = chosen_direction;
            ui.label(theme::muted("positive scrolls down or right", 9.5));
        }
    }
}

fn modifier_toggle(ui: &mut Ui, value: &mut bool, label: &str) {
    if ui
        .selectable_label(*value, theme::body(label, 10.5))
        .clicked()
    {
        *value = !*value;
    }
}

fn convert_rate(rate: f64, from: RateUnit, to: RateUnit) -> f64 {
    let converted = match (from, to) {
        (RateUnit::PerSecond, RateUnit::PerMinute) => rate * 60.0,
        (RateUnit::PerMinute, RateUnit::PerSecond) => rate / 60.0,
        _ => rate,
    };
    match to {
        RateUnit::PerSecond => converted.clamp(0.01, 1_000.0),
        RateUnit::PerMinute => converted.clamp(1.0, 60_000.0),
    }
}

fn trim_number(value: f64, decimals: usize) -> String {
    let mut formatted = format!("{value:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    formatted
}

fn metric_decimals(value: f64) -> usize {
    if value >= 1_000.0 {
        0
    } else if value >= 100.0 {
        1
    } else if value >= 1.0 {
        2
    } else {
        3
    }
}

fn grouped_number(value: f64, decimals: usize) -> String {
    let raw = trim_number(value, decimals);
    let (integer, fraction) = match raw.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (raw.as_str(), None),
    };

    let mut grouped = String::new();
    let chars = integer.chars().rev().collect::<Vec<_>>();
    for (index, character) in chars.iter().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*character);
    }
    let integer = grouped.chars().rev().collect::<String>();
    match fraction {
        Some(fraction) if !fraction.is_empty() => format!("{integer}.{fraction}"),
        _ => integer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_convert_between_units() {
        assert!(
            (convert_rate(10.0, RateUnit::PerSecond, RateUnit::PerMinute) - 600.0).abs() < 1e-9
        );
        assert!(
            (convert_rate(600.0, RateUnit::PerMinute, RateUnit::PerSecond) - 10.0).abs() < 1e-9
        );
    }

    #[test]
    fn numbers_are_grouped_and_trimmed() {
        assert_eq!(grouped_number(10.0, 2), "10");
        assert_eq!(grouped_number(12.5, 2), "12.5");
        assert_eq!(grouped_number(60_000.0, 0), "60,000");
    }
}
