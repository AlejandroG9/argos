mod app;
mod jump;
mod theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 650.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Argos",
        options,
        Box::new(|_cc| Ok(Box::new(app::ArgosApp::new()))),
    )
}
