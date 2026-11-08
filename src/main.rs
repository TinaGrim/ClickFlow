#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod engine;
mod model;
mod platform;
mod theme;

use app::AutoClickerApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([1040.0, 650.0])
            .with_title("ClickFlow - Auto Clicker")
            .with_icon(app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "ClickFlow",
        options,
        Box::new(|cc| Ok(Box::new(AutoClickerApp::new(cc)))),
    )
}

fn app_icon() -> eframe::egui::IconData {
    const SIZE: u32 = 64;
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let index = ((y * SIZE + x) * 4) as usize;
            let dx = x as f32 - 31.5;
            let dy = y as f32 - 31.5;
            let distance = (dx * dx + dy * dy).sqrt();

            let (r, g, b, a) = if distance <= 26.0 {
                if (distance - 15.0).abs() <= 2.1 || distance <= 3.3 {
                    (24, 19, 9, 255)
                } else {
                    (218, 168, 73, 255)
                }
            } else {
                (15, 19, 21, 0)
            };
            rgba[index] = r;
            rgba[index + 1] = g;
            rgba[index + 2] = b;
            rgba[index + 3] = a;
        }
    }

    eframe::egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}
