mod app;
mod problem;
mod spaced_rep;
mod storage;

use app::TimesTablesApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 420.0])
            .with_min_inner_size([350.0, 380.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Times Tables Practice",
        options,
        Box::new(|cc| Ok(Box::new(TimesTablesApp::new(cc)))),
    )
}
