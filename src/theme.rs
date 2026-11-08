use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Vec2, Visuals,
};

pub const BG: Color32 = Color32::from_rgb(15, 19, 21);
pub const PANEL: Color32 = Color32::from_rgb(20, 25, 28);
pub const SURFACE: Color32 = Color32::from_rgb(25, 31, 35);
pub const FIELD: Color32 = Color32::from_rgb(30, 37, 42);
pub const FIELD_HOVER: Color32 = Color32::from_rgb(36, 44, 49);
pub const BORDER: Color32 = Color32::from_rgb(43, 52, 58);
pub const BORDER_SOFT: Color32 = Color32::from_rgb(34, 42, 47);
pub const TEXT: Color32 = Color32::from_rgb(234, 230, 220);
pub const MUTED: Color32 = Color32::from_rgb(143, 154, 158);
pub const DIM: Color32 = Color32::from_rgb(98, 109, 114);
pub const ACCENT: Color32 = Color32::from_rgb(218, 168, 73);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(117, 91, 43);
pub const INK: Color32 = Color32::from_rgb(24, 19, 9);
pub const SUCCESS: Color32 = Color32::from_rgb(143, 191, 159);
pub const SUCCESS_DIM: Color32 = Color32::from_rgb(53, 83, 64);
pub const DANGER: Color32 = Color32::from_rgb(218, 124, 108);
pub const DANGER_BG: Color32 = Color32::from_rgb(49, 29, 27);
pub const WARN_BG: Color32 = Color32::from_rgb(48, 40, 24);

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "plex_sans".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "plex_sans_semibold".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf")).into(),
    );
    fonts.font_data.insert(
        "plex_mono".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf")).into(),
    );

    let proportional_fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let monospace_fallback = fonts
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let mut sans = vec!["plex_sans".to_owned()];
    sans.extend(proportional_fallback.clone());
    fonts.families.insert(FontFamily::Proportional, sans);

    let mut semibold = vec!["plex_sans_semibold".to_owned()];
    semibold.extend(proportional_fallback);
    fonts.families.insert(semibold_family(), semibold);

    let mut mono = vec!["plex_mono".to_owned()];
    mono.extend(monospace_fallback);
    fonts.families.insert(FontFamily::Monospace, mono);

    ctx.set_fonts(fonts);

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = SURFACE;
    visuals.code_bg_color = FIELD;
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = ACCENT;
    visuals.error_fg_color = DANGER;
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.button_frame = true;
    visuals.striped = false;
    visuals.slider_trailing_fill = true;

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SOFT);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(7);

    visuals.widgets.inactive.bg_fill = FIELD;
    visuals.widgets.inactive.weak_bg_fill = FIELD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(7);

    visuals.widgets.hovered.bg_fill = FIELD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = FIELD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_DIM);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(7);

    visuals.widgets.active.bg_fill = FIELD_HOVER;
    visuals.widgets.active.weak_bg_fill = FIELD_HOVER;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.active.corner_radius = CornerRadius::same(7);

    visuals.widgets.open.bg_fill = SURFACE;
    visuals.widgets.open.weak_bg_fill = SURFACE;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.open.corner_radius = CornerRadius::same(7);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.animation_time = 0.12;
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(11.0, 6.0);
    style.spacing.interact_size = Vec2::new(40.0, 30.0);
    style.spacing.slider_width = 140.0;
    style.spacing.indent = 18.0;

    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(21.0, semibold_family()));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );

    ctx.set_style(style);
}

pub fn semibold_family() -> FontFamily {
    FontFamily::Name("plex_semibold".into())
}

pub fn body(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, FontFamily::Proportional))
}

pub fn semibold(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, semibold_family()))
}

pub fn mono(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, FontFamily::Monospace))
}

pub fn muted(text: impl Into<String>, size: f32) -> egui::RichText {
    body(text, size).color(MUTED)
}

pub fn hairline(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, BORDER_SOFT);
    ui.add_space(8.0);
}
