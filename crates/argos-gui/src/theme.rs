use argos_core::model::{AgentState, Confidence};
use egui::Color32;

/// Paleta corta y contenida. Lo que hace que una interfaz se vea cuidada no
/// es el renderizador sino la disciplina: pocos colores, una escala de
/// espaciado, y jerarquía tipográfica real.
pub mod color {
    use egui::Color32;

    pub const FONDO: Color32 = Color32::from_rgb(0x14, 0x16, 0x1A);
    pub const SUPERFICIE: Color32 = Color32::from_rgb(0x1C, 0x1F, 0x26);
    pub const SUPERFICIE_ALTA: Color32 = Color32::from_rgb(0x26, 0x2A, 0x33);
    pub const BORDE: Color32 = Color32::from_rgb(0x2E, 0x33, 0x3D);
    pub const TEXTO: Color32 = Color32::from_rgb(0xE4, 0xE7, 0xEC);
    pub const TEXTO_TENUE: Color32 = Color32::from_rgb(0x8B, 0x93, 0xA1);
    pub const ACENTO: Color32 = Color32::from_rgb(0x5B, 0x8D, 0xEF);
    pub const LINEA: Color32 = Color32::from_rgb(0x39, 0x3F, 0x4B);
}

/// Escala de espaciado. Usar siempre estos valores y no números sueltos es
/// la mitad de por qué una interfaz se ve ordenada.
pub mod espacio {
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

pub const REDONDEO: f32 = 8.0;

/// Una letra por plataforma para la insignia del nodo. No son los logotipos
/// reales —eso necesitaría empaquetar imágenes— pero distinguen de un
/// vistazo, que es lo que hace falta a este tamaño.
pub fn archivo_de_logo(c: argos_core::model::ClientKind) -> &'static str {
    use argos_core::model::ClientKind;
    match c {
        ClientKind::ClaudeCode => "claude",
        ClientKind::Codex => "codex",
        ClientKind::GeminiCli => "gemini",
        ClientKind::Antigravity => "agy",
    }
}

/// Carpeta donde el usuario deja los logotipos. Son marcas de terceros, así
/// que no viajan con la app: si el archivo está, se usa; si no, la inicial.
pub fn carpeta_de_logos() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join(".argos/logos")
}

pub fn inicial_de_cliente(c: argos_core::model::ClientKind) -> &'static str {
    use argos_core::model::ClientKind;
    match c {
        ClientKind::ClaudeCode => "C",
        ClientKind::Codex => "X",
        ClientKind::GeminiCli => "G",
        ClientKind::Antigravity => "A",
    }
}

/// Un color por carril para poder seguir una rama con la vista. Se repiten
/// al agotarse: más de seis ramas simultáneas ya no se distinguen por color
/// por muchos que añadas.
pub fn color_de_carril(carril: usize) -> Color32 {
    const CARRILES: [Color32; 6] = [
        Color32::from_rgb(0x5B, 0x8D, 0xEF),
        Color32::from_rgb(0x3C, 0xAA, 0x6E),
        Color32::from_rgb(0xE0, 0xA0, 0x30),
        Color32::from_rgb(0xC0, 0x7C, 0xD8),
        Color32::from_rgb(0x4C, 0xB5, 0xC0),
        Color32::from_rgb(0xD8, 0x6E, 0x6E),
    ];
    CARRILES[carril % CARRILES.len()]
}

/// "1 sesiones" delata descuido en una interfaz que presume de cuidada.
pub fn plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("{n} {singular}")
    } else {
        format!("{n} {plural}")
    }
}

/// Aplica la identidad visual al contexto. Sin esto, egui se ve como egui.
pub fn aplicar_estilo(ctx: &egui::Context) {
    let mut estilo = (*ctx.style()).clone();

    estilo.visuals.dark_mode = true;
    estilo.visuals.panel_fill = color::FONDO;
    estilo.visuals.window_fill = color::FONDO;
    estilo.visuals.extreme_bg_color = color::FONDO;
    estilo.visuals.override_text_color = Some(color::TEXTO);

    let r = egui::CornerRadius::same(REDONDEO as u8);
    for w in [
        &mut estilo.visuals.widgets.noninteractive,
        &mut estilo.visuals.widgets.inactive,
        &mut estilo.visuals.widgets.hovered,
        &mut estilo.visuals.widgets.active,
        &mut estilo.visuals.widgets.open,
    ] {
        w.corner_radius = r;
    }
    estilo.visuals.widgets.inactive.weak_bg_fill = color::SUPERFICIE;
    estilo.visuals.widgets.hovered.weak_bg_fill = color::SUPERFICIE_ALTA;
    estilo.visuals.widgets.active.weak_bg_fill = color::SUPERFICIE_ALTA;
    estilo.visuals.selection.bg_fill = color::ACENTO.gamma_multiply(0.35);

    estilo.spacing.item_spacing = egui::vec2(espacio::S, espacio::S);
    estilo.spacing.button_padding = egui::vec2(espacio::M, espacio::S);
    estilo.spacing.window_margin = egui::Margin::same(espacio::L as i8);

    use egui::{FontFamily::Proportional, FontId, TextStyle};
    estilo.text_styles = [
        (TextStyle::Heading, FontId::new(19.0, Proportional)),
        (TextStyle::Body, FontId::new(13.5, Proportional)),
        (TextStyle::Button, FontId::new(13.0, Proportional)),
        (TextStyle::Small, FontId::new(11.5, Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(12.5, egui::FontFamily::Monospace),
        ),
    ]
    .into();

    ctx.set_style(estilo);
}

/// Símbolo y color por estado. El símbolo es obligatorio: el color solo
/// refuerza, nunca es la única señal (spec §8).
pub fn state_badge(state: AgentState) -> (&'static str, Color32) {
    match state {
        AgentState::Waiting => ("◆", Color32::from_rgb(230, 160, 30)),
        AgentState::Working => ("▶", Color32::from_rgb(60, 170, 110)),
        AgentState::Unknown => ("?", Color32::from_rgb(140, 140, 150)),
        AgentState::Finished => ("✓", Color32::from_rgb(95, 110, 130)),
    }
}

pub fn state_label(state: AgentState) -> &'static str {
    match state {
        AgentState::Waiting => "esperando respuesta",
        AgentState::Working => "trabajando",
        AgentState::Unknown => "desconocido",
        AgentState::Finished => "terminó",
    }
}

/// Una antigüedad en segundos crudos ("hace 96423s") es ilegible de un
/// vistazo, que es justo lo que el tablero promete.
pub fn edad_legible(segundos: i64) -> String {
    let s = segundos.max(0);
    match s {
        0..=59 => format!("hace {s}s"),
        60..=3599 => format!("hace {}m", s / 60),
        3600..=86399 => format!("hace {}h", s / 3600),
        _ => format!("hace {}d", s / 86400),
    }
}

/// Una inferencia dudosa se muestra como dudosa (spec §4).
pub fn confidence_hint(confidence: Confidence) -> Option<&'static str> {
    match confidence {
        Confidence::High => None,
        Confidence::Medium => Some("~"),
        Confidence::Low => Some("?"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argos_core::model::AgentState;

    /// El spec §8 pide color **y** forma: un indicador que dependa solo del
    /// color es inservible para quien no distingue esos colores.
    #[test]
    fn cada_estado_tiene_simbolo_propio_ademas_de_color() {
        let simbolos: Vec<&str> = [
            AgentState::Waiting,
            AgentState::Working,
            AgentState::Unknown,
            AgentState::Finished,
        ]
        .into_iter()
        .map(|s| state_badge(s).0)
        .collect();

        let unicos: std::collections::HashSet<_> = simbolos.iter().collect();
        assert_eq!(unicos.len(), 4, "los símbolos deben distinguirse entre sí");
    }

    /// "hace 96423s" es lo que mostraba antes: 26 horas en segundos crudos.
    #[test]
    fn la_edad_se_muestra_en_la_unidad_que_se_lee_de_un_vistazo() {
        assert_eq!(edad_legible(5), "hace 5s");
        assert_eq!(edad_legible(59), "hace 59s");
        assert_eq!(edad_legible(60), "hace 1m");
        assert_eq!(edad_legible(3599), "hace 59m");
        assert_eq!(edad_legible(3600), "hace 1h");
        assert_eq!(edad_legible(96423), "hace 1d");
        assert_eq!(edad_legible(310802), "hace 3d");
    }

    #[test]
    fn una_edad_negativa_por_desfase_de_reloj_no_se_muestra_absurda() {
        assert_eq!(edad_legible(-10), "hace 0s");
    }

    #[test]
    fn el_singular_y_el_plural_se_escriben_como_toca() {
        assert_eq!(plural(0, "sesión", "sesiones"), "0 sesiones");
        assert_eq!(plural(1, "sesión", "sesiones"), "1 sesión");
        assert_eq!(plural(2, "sesión", "sesiones"), "2 sesiones");
    }

    #[test]
    fn los_colores_tambien_se_distinguen() {
        let a = state_badge(AgentState::Waiting).1;
        let b = state_badge(AgentState::Working).1;
        assert_ne!(a, b);
    }
}
