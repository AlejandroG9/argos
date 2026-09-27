use argos_core::model::ClientKind;
use std::collections::HashMap;

/// Logotipos que el usuario dejó en su carpeta. Son marcas de terceros, así
/// que no viajan con la app: si el archivo no está, el nodo muestra la
/// inicial y no pasa nada.
#[derive(Default)]
pub struct Logos {
    texturas: HashMap<String, Option<egui::TextureHandle>>,
}

impl Logos {
    /// Carga perezosa y recordada, incluido el fallo: un logo que no existe
    /// no se reintenta en cada cuadro.
    pub fn textura(
        &mut self,
        ctx: &egui::Context,
        cliente: ClientKind,
    ) -> Option<&egui::TextureHandle> {
        let nombre = crate::theme::archivo_de_logo(cliente).to_string();

        self.texturas
            .entry(nombre.clone())
            .or_insert_with(|| cargar(ctx, &nombre))
            .as_ref()
    }
}

fn cargar(ctx: &egui::Context, nombre: &str) -> Option<egui::TextureHandle> {
    let ruta = crate::theme::carpeta_de_logos().join(format!("{nombre}.png"));
    let bytes = std::fs::read(&ruta).ok()?;
    let imagen = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let tamano = [imagen.width() as usize, imagen.height() as usize];

    Some(ctx.load_texture(
        format!("logo-{nombre}"),
        egui::ColorImage::from_rgba_unmultiplied(tamano, imagen.as_raw()),
        egui::TextureOptions::LINEAR,
    ))
}
