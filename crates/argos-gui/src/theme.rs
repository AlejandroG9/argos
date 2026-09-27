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
    pub const LINEA: Color32 = Color32::from_rgb(0x39, 0x3F, 0x4B);

    /// Cobre. **Solo identidad**: el icono, la marca de la barra, el .icns.
    /// Comparte familia de tono con el ámbar de *esperando*, así que no puede
    /// hacer de cromo interactivo sin competir con un estado.
    pub const MARCA: Color32 = Color32::from_rgb(0xB8, 0x73, 0x33);

    /// Hueso: el acento de interfaz —lo seleccionado, lo enfocado—.
    /// Acromático a propósito: sin tono no se parece a ningún estado, ni
    /// ahora ni cuando se añada un séptimo carril.
    pub const ACENTO: Color32 = Color32::from_rgb(0xE8, 0xE3, 0xD7);

    /// Texto sobre un relleno de acento. El hueso es claro: el texto del tema
    /// encima sería ilegible.
    pub const SOBRE_ACENTO: Color32 = Color32::from_rgb(0x0F, 0x11, 0x14);
}

/// Escala de espaciado. Usar siempre estos valores y no números sueltos es
/// la mitad de por qué una interfaz se ve ordenada.
pub mod espacio {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

pub const REDONDEO: f32 = 8.0;

/// Un vacío explicado, centrado y con la salida a mano. "No hay nada" sin
/// decir por qué ni qué hacer deja al usuario preguntándose si se rompió.
pub fn estado_vacio(ui: &mut egui::Ui, titulo: &str, pista: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(espacio::XL * 2.0);
        ui.label(egui::RichText::new(titulo).size(15.0).color(color::TEXTO));
        ui.add_space(espacio::S);
        ui.label(
            egui::RichText::new(pista)
                .size(12.5)
                .color(color::TEXTO_TENUE),
        );
    });
}

/// Un chip: relleno de acento y texto oscuro cuando está activo, texto tenue
/// cuando no.
///
/// Existe porque `selectable_value` pinta el relleno de selección pero deja el
/// texto del tema, que sobre el hueso claro queda ilegible. El chip también es
/// la forma que tiene el diseño: pastilla redondeada, no botón.
pub fn chip(ui: &mut egui::Ui, texto: &str, activo: bool) -> egui::Response {
    let fuente = egui::FontId::new(12.5, egui::FontFamily::Proportional);
    // PLACEHOLDER deja el color para el pintado: así el mismo trazado sirve
    // para los tres estados sin volver a medir el texto.
    let galley = ui
        .painter()
        .layout_no_wrap(texto.to_owned(), fuente, egui::Color32::PLACEHOLDER);

    let relleno = egui::vec2(espacio::M - 1.0, espacio::XS + 1.0);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + relleno * 2.0, egui::Sense::click());

    let (fondo, tinta) = match (activo, resp.hovered()) {
        (true, _) => (color::ACENTO, color::SOBRE_ACENTO),
        (false, true) => (color::SUPERFICIE_ALTA, color::TEXTO),
        (false, false) => (egui::Color32::TRANSPARENT, color::TEXTO_TENUE),
    };

    ui.painter().rect_filled(rect, REDONDEO - 1.0, fondo);
    ui.painter().galley(rect.min + relleno, galley, tinta);

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Un chip que además fija el valor al pulsarse, como `selectable_value`.
pub fn chip_valor<T: PartialEq>(
    ui: &mut egui::Ui,
    actual: &mut T,
    valor: T,
    texto: &str,
) -> egui::Response {
    let resp = chip(ui, texto, *actual == valor);
    if resp.clicked() {
        *actual = valor;
    }
    resp
}

/// Una fila de lista de ancho completo: nombre a la izquierda, dato tenue a
/// la derecha.
///
/// El realce va en el fondo y no en un borde: un borde por fila convierte una
/// lista en una reja, y a veinte proyectos eso es todo lo que se ve.
pub fn fila(ui: &mut egui::Ui, nombre: &str, derecha: &str) -> egui::Response {
    let alto = 38.0;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), alto), egui::Sense::click());

    if resp.hovered() {
        ui.painter().rect_filled(rect, REDONDEO, color::SUPERFICIE);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    ui.painter().text(
        egui::pos2(rect.left() + espacio::M, rect.center().y),
        egui::Align2::LEFT_CENTER,
        nombre,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
        color::TEXTO,
    );
    ui.painter().text(
        egui::pos2(rect.right() - espacio::M, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        derecha,
        egui::FontId::new(11.5, egui::FontFamily::Monospace),
        color::TEXTO_TENUE,
    );

    resp
}

/// La ruta con `~` en vez del home. La ruta completa de un proyecto es casi
/// toda prefijo repetido: lo que distingue está al final.
pub fn ruta_corta(ruta: &std::path::Path) -> String {
    let texto = ruta.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && texto.starts_with(&home) => {
            format!("~{}", &texto[home.len()..])
        }
        _ => texto,
    }
}

/// La marca, dibujada en vez de cargada.
///
/// A tamaño de barra la variante de cuatro nodos es la legible —los ocho se
/// empastan por debajo de 48 px— y dibujarla la mantiene nítida en cualquier
/// pantalla sin empaquetar un PNG por densidad. Es el único sitio de la
/// interfaz donde aparece el cobre.
pub fn pintar_marca(pintor: &egui::Painter, centro: egui::Pos2, radio: f32) {
    let brazo = radio * 0.68;
    let nodo = radio * 0.2;

    for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
        let punta = centro + egui::vec2(dx * brazo, dy * brazo);
        pintor.line_segment(
            [centro, punta],
            egui::Stroke::new(radio * 0.1, color::LINEA),
        );
        pintor.circle_filled(punta, nodo, color::MARCA);
    }

    pintor.circle_filled(centro, radio * 0.42, color::MARCA);
    pintor.circle_stroke(
        centro,
        radio * 0.42,
        egui::Stroke::new(radio * 0.11, color::FONDO),
    );
}

/// Una barra con fondo propio separa el mando del lienzo. Sin ese contraste
/// el árbol parece flotar y los controles se confunden con el contenido.
pub fn pintar_barra(ui: &mut egui::Ui) {
    let r = ui.max_rect().expand2(egui::vec2(espacio::XL, 0.0));
    ui.painter().rect_filled(r, 0.0, color::SUPERFICIE);
    ui.painter()
        .hline(r.x_range(), r.max.y, egui::Stroke::new(1.0, color::BORDE));
}

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
        // El encabezado va en semibold, no en el mismo peso teñido de otro
        // color: la jerarquía la hace el peso.
        (TextStyle::Heading, crate::tipografia::fuerte(19.0)),
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

/// Símbolo y color por estado. La paleta y su porqué están en
/// `docs/diseno.md`.
/// El símbolo es obligatorio: el color solo
/// refuerza, nunca es la única señal (spec §8).
pub fn state_badge(state: AgentState) -> (&'static str, Color32) {
    match state {
        AgentState::Waiting => ("◆", Color32::from_rgb(0xE6, 0xA0, 0x1E)),
        AgentState::Working => ("▶", Color32::from_rgb(0x3C, 0xAA, 0x6E)),
        AgentState::Unknown => ("?", Color32::from_rgb(0x8C, 0x8C, 0x96)),
        AgentState::Finished => ("✓", Color32::from_rgb(0x5F, 0x6E, 0x82)),
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

    /// Luminancia relativa (WCAG). Sirve para comprobar que dos colores se
    /// distinguen aunque quien mire no perciba el tono.
    fn luma(c: Color32) -> f32 {
        let lineal = |v: u8| {
            let s = v as f32 / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lineal(c.r()) + 0.7152 * lineal(c.g()) + 0.0722 * lineal(c.b())
    }

    /// Por esto el acento no es ámbar ni verde: esos dos ya significan
    /// *esperando* y *trabajando*, y el cromo aparece junto a ellos. Un acento
    /// que se lee como un estado es un error de lectura, no una decoración.
    #[test]
    fn el_acento_de_interfaz_no_se_confunde_con_ningun_estado() {
        for estado in [
            AgentState::Waiting,
            AgentState::Working,
            AgentState::Unknown,
            AgentState::Finished,
        ] {
            let color = state_badge(estado).1;
            let distancia = (luma(color::ACENTO) - luma(color)).abs();
            assert!(
                distancia > 0.25,
                "{estado:?} ({color:?}) está a {distancia:.3} del acento: \
                 se distinguirían solo por tono"
            );
        }
    }

    /// El precio del cobre: es identidad (icono, marca) y nunca cromo
    /// interactivo, porque comparte familia de tono con el ámbar de
    /// *esperando*. Igualarlos reintroduce el choque sin que se note.
    #[test]
    fn la_marca_no_hace_de_acento_de_interfaz() {
        assert_ne!(
            color::MARCA,
            color::ACENTO,
            "el cobre es identidad; el cromo es hueso (ver docs/diseno.md)"
        );
    }
}
