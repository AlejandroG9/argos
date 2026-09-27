//! Los tipos del diseño, empaquetados en el binario.
//!
//! Sin esto egui se ve como egui: su tipo por defecto no es ninguno de los
//! tres que fija `docs/diseno.md`, y la app se parece a cualquier otra app de
//! egui en vez de a su propio diseño. Son OFL, así que viajan con nosotros
//! (las licencias están junto a los archivos en `assets/fonts/`).

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

/// Instrument Serif: títulos y la palabra "Argos". Un display con carácter
/// junto a un cuerpo neutro es lo que separa una interfaz con identidad de
/// una plantilla.
pub const DISPLAY: &str = "display";

/// IBM Plex Sans SemiBold. La jerarquía la hace el peso, no el color: por eso
/// hay una familia propia en vez de teñir el texto.
pub const FUERTE: &str = "fuerte";

/// El tipo que egui trae para monoespaciado. Es el único de todos los que
/// hay a mano —los nuestros incluidos— que tiene las formas geométricas
/// ◆ ● ■ ▲, y esas formas son los símbolos de estado.
const PORTADOR_DE_SIMBOLOS: &str = "Hack";

/// Se instala una vez, al arrancar. Cada `set_fonts` reconstruye el atlas de
/// glifos, así que llamarlo por fotograma costaría lo que cuesta dibujar.
pub fn instalar(ctx: &egui::Context) {
    ctx.set_fonts(definiciones());
}

/// Separado de `instalar` para poder comprobar las cadenas de respaldo sin
/// levantar una ventana: que un glifo salga como cuadrito no se ve en un
/// test de compilación, solo mirando la app.
pub fn definiciones() -> FontDefinitions {
    let mut f = FontDefinitions::default();

    let mut meter = |nombre: &str, bytes: &'static [u8]| {
        f.font_data
            .insert(nombre.to_owned(), Arc::new(FontData::from_static(bytes)));
    };

    // Light (300), no Regular (400). Sobre fondo oscuro el texto claro
    // florece y un peso normal se lee espeso: por eso el propio egui trae
    // una Ubuntu **Light** de origen. Cambiarla por una Regular fue lo que
    // dejó la interfaz con la letra saturada.
    meter(
        "plex-sans",
        include_bytes!("../../../assets/fonts/IBMPlexSans-Light.ttf"),
    );
    meter(
        "plex-sans-semibold",
        include_bytes!("../../../assets/fonts/IBMPlexSans-SemiBold.ttf"),
    );
    meter(
        "plex-mono",
        include_bytes!("../../../assets/fonts/IBMPlexMono-Regular.ttf"),
    );
    meter(
        "instrument-serif",
        include_bytes!("../../../assets/fonts/InstrumentSerif-Regular.ttf"),
    );

    // Se antepone en vez de reemplazar: la interfaz usa ◆ ▶ ✓ ✕ ← y no todos
    // están en Plex. egui recorre la lista por glifo, así que lo que falte
    // cae en los tipos de siempre en lugar de salir como un cuadrito.
    let proporcional = f.families.entry(FontFamily::Proportional).or_default();
    proporcional.insert(0, "plex-sans".to_owned());

    // Y al final el portador de símbolos. Sin esto, ◆ —el estado *esperando*,
    // el más importante que tiene la app— sale como cuadrito: ninguno de los
    // tipos proporcionales, ni los nuestros ni los de egui, lo tiene.
    if !proporcional.iter().any(|n| n == PORTADOR_DE_SIMBOLOS) {
        proporcional.push(PORTADOR_DE_SIMBOLOS.to_owned());
    }

    f.families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "plex-mono".to_owned());

    let respaldo = f
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let con_respaldo = |primero: &str| {
        let mut v = vec![primero.to_owned()];
        v.extend(respaldo.iter().cloned());
        v
    };

    f.families.insert(
        FontFamily::Name(DISPLAY.into()),
        con_respaldo("instrument-serif"),
    );
    f.families.insert(
        FontFamily::Name(FUERTE.into()),
        con_respaldo("plex-sans-semibold"),
    );

    f
}

/// Un tamaño en el display de la marca.
pub fn display(tamano: f32) -> egui::FontId {
    egui::FontId::new(tamano, FontFamily::Name(DISPLAY.into()))
}

/// Un tamaño en el cuerpo semibold.
pub fn fuerte(tamano: f32) -> egui::FontId {
    egui::FontId::new(tamano, FontFamily::Name(FUERTE.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cadena(f: &FontDefinitions, familia: FontFamily) -> Vec<String> {
        f.families.get(&familia).cloned().unwrap_or_default()
    }

    /// El texto normal es Plex; lo que Plex no tenga cae hacia atrás.
    #[test]
    fn el_cuerpo_empieza_por_plex_y_conserva_los_respaldos() {
        let f = definiciones();
        let c = cadena(&f, FontFamily::Proportional);

        assert_eq!(c.first().map(String::as_str), Some("plex-sans"));
        assert!(c.len() > 1, "sin respaldo, un glifo que falte sale vacío");
    }

    /// ◆ es el estado *esperando*, la señal más importante de la app, y solo
    /// lo trae el tipo monoespaciado de egui. Si sale de la cadena
    /// proporcional, vuelve a verse como un cuadrito.
    #[test]
    fn la_cadena_del_cuerpo_termina_en_el_portador_de_simbolos() {
        let c = cadena(&definiciones(), FontFamily::Proportional);

        assert!(
            c.contains(&PORTADOR_DE_SIMBOLOS.to_owned()),
            "falta {PORTADOR_DE_SIMBOLOS} en la cadena proporcional: {c:?}"
        );
    }

    /// Las familias propias heredan la misma cadena, símbolos incluidos: el
    /// encabezado también muestra estados.
    #[test]
    fn las_familias_propias_heredan_el_respaldo_completo() {
        let f = definiciones();

        for familia in [
            FontFamily::Name(DISPLAY.into()),
            FontFamily::Name(FUERTE.into()),
        ] {
            let c = cadena(&f, familia.clone());
            assert!(
                c.contains(&PORTADOR_DE_SIMBOLOS.to_owned()),
                "{familia:?} se quedó sin símbolos: {c:?}"
            );
        }
    }
}
