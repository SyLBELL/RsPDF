#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 840.0])
            .with_min_inner_size([980.0, 600.0])
            .with_drag_and_drop(true)
            .with_title("PDF Studio"),
        ..Default::default()
    };
    eframe::run_native("PDF Studio", opts, Box::new(|cc| Ok(Box::new(ui::App::new(cc, std::env::args().skip(1).collect())))))
}
