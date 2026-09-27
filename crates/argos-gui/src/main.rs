mod app;
mod git_vista;
mod jump;
mod logos;
mod mascota;
mod nodos;
mod projects;
mod selector;
mod theme;
mod tipografia;
mod ventana;

/// El icono de la ventana y del Dock.
///
/// Es `argos-icono.png` y no la marca a secas: la marca llena su lienzo, y un
/// icono que llena el suyo se ve enorme junto a los demás del Dock, que todos
/// dejan margen. El icono lleva la baldosa metida como pide macOS.
fn icono() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../../assets/argos-icono.png");
    let imagen = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = imagen.dimensions();
    Some(egui::IconData {
        rgba: imagen.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([900.0, 650.0]);
    if let Some(i) = icono() {
        viewport = viewport.with_icon(i);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Argos",
        options,
        Box::new(|cc| {
            // Una vez, al arrancar: cada cambio de tipos reconstruye el atlas
            // de glifos, así que no es algo para hacer por fotograma.
            tipografia::instalar(&cc.egui_ctx);
            Ok(Box::new(app::ArgosApp::new()))
        }),
    )
}
