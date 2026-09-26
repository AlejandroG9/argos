use crate::app::Filter;
use crate::ventana::Ventana;
use argos_core::model::AgentState;
use argos_core::store::SessionRow;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Separación vertical entre ramas, en unidades de fila.
const AIRE_ENTRE_RAMAS: f32 = 0.6;

pub struct Nodo {
    pub id: String,
    pub etiqueta: String,
    pub detalle: String,
    pub estado: AgentState,
    /// 0 = rama, 1 = sesión, 2 = subagente.
    pub columna: usize,
    /// Posición vertical en unidades de fila; el pintor la escala a píxeles.
    pub fila: f32,
    /// `None` en los nodos de rama, que no corresponden a una sesión.
    pub session_id: Option<String>,
}

#[derive(Default)]
pub struct Grafo {
    pub nodos: Vec<Nodo>,
    pub aristas: Vec<(String, String)>,
}

impl Grafo {
    /// Cuántas filas ocupa el árbol, para dimensionar el lienzo.
    pub fn filas_totales(&self) -> f32 {
        self.nodos.iter().map(|n| n.fila).fold(0.0_f32, f32::max) + 1.0
    }
}

/// Coloca el árbol proyecto → rama → sesión → subagente en una rejilla.
///
/// El layout es determinista, no una simulación: los datos son un árbol, así
/// que basta recorrerlo asignando filas a las hojas y centrando a cada padre
/// entre sus hijos. Eso es lo que evita que las líneas se crucen.
pub fn construir_grafo(
    rows: &[SessionRow],
    filter: Filter,
    ventana: Ventana,
    now: DateTime<Utc>,
) -> Grafo {
    let visibles: Vec<&SessionRow> = rows
        .iter()
        .filter(|r| filter.acepta(r.state) && ventana.acepta(r.last_activity, now))
        .collect();

    let mut por_rama: BTreeMap<String, Vec<&SessionRow>> = BTreeMap::new();
    for row in &visibles {
        let clave = row
            .branch
            .clone()
            .unwrap_or_else(|| format!("(sin rama) {}", row.anchor_path.display()));
        por_rama.entry(clave).or_default().push(row);
    }

    // Las ramas se ordenan por urgencia, igual que en el resto de la app.
    let mut ramas: Vec<(String, Vec<&SessionRow>)> = por_rama.into_iter().collect();
    ramas.sort_by(|(na, a), (nb, b)| {
        urgencia_minima(a)
            .cmp(&urgencia_minima(b))
            .then_with(|| na.cmp(nb))
    });

    let mut grafo = Grafo::default();
    let mut siguiente_fila = 0.0_f32;

    for (rama, filas) in ramas {
        let id_rama = format!("rama:{rama}");

        let mut raices: Vec<&&SessionRow> =
            filas.iter().filter(|r| r.parent_id.is_none()).collect();
        raices.sort_by_key(|r| (r.state.urgency(), r.id.clone()));

        let mut filas_de_sesiones = Vec::new();

        for sesion in raices {
            let mut hijos: Vec<&&SessionRow> = filas
                .iter()
                .filter(|r| r.parent_id.as_deref() == Some(sesion.id.as_str()))
                .collect();
            hijos.sort_by_key(|r| (r.state.urgency(), r.id.clone()));

            let mut filas_de_hijos = Vec::new();
            for hijo in &hijos {
                grafo
                    .nodos
                    .push(nodo_de_sesion(hijo, 2, siguiente_fila, now));
                grafo.aristas.push((sesion.id.clone(), hijo.id.clone()));
                filas_de_hijos.push(siguiente_fila);
                siguiente_fila += 1.0;
            }

            let fila_sesion = if filas_de_hijos.is_empty() {
                let f = siguiente_fila;
                siguiente_fila += 1.0;
                f
            } else {
                promedio(&filas_de_hijos)
            };

            grafo
                .nodos
                .push(nodo_de_sesion(sesion, 1, fila_sesion, now));
            grafo.aristas.push((id_rama.clone(), sesion.id.clone()));
            filas_de_sesiones.push(fila_sesion);
        }

        if filas_de_sesiones.is_empty() {
            continue;
        }

        grafo.nodos.push(Nodo {
            id: id_rama,
            etiqueta: rama,
            detalle: crate::theme::plural(filas_de_sesiones.len(), "sesión", "sesiones"),
            estado: estado_mas_urgente(&filas),
            columna: 0,
            fila: promedio(&filas_de_sesiones),
            session_id: None,
        });

        siguiente_fila += AIRE_ENTRE_RAMAS;
    }

    grafo
}

fn nodo_de_sesion(row: &SessionRow, columna: usize, fila: f32, now: DateTime<Utc>) -> Nodo {
    use crate::theme::{confidence_hint, edad_legible, state_label};

    // El nodo carga lo mismo que mostraba la fila del árbol: estado, la duda
    // cuando la hay, y hace cuánto. Cambiar de vista no debe perder datos.
    let detalle = format!(
        "{}{} · {}",
        state_label(row.state),
        confidence_hint(row.confidence).unwrap_or(""),
        edad_legible((now - row.last_activity).num_seconds()),
    );

    Nodo {
        id: row.id.clone(),
        etiqueta: row.client.label().to_string(),
        detalle,
        estado: row.state,
        columna,
        fila,
        session_id: Some(row.id.clone()),
    }
}

fn urgencia_minima(filas: &[&SessionRow]) -> u8 {
    filas
        .iter()
        .map(|r| r.state.urgency())
        .min()
        .unwrap_or(u8::MAX)
}

fn estado_mas_urgente(filas: &[&SessionRow]) -> AgentState {
    filas
        .iter()
        .map(|r| r.state)
        .min_by_key(|s| s.urgency())
        .unwrap_or(AgentState::Unknown)
}

fn promedio(valores: &[f32]) -> f32 {
    valores.iter().sum::<f32>() / valores.len() as f32
}

// --- pintado ---------------------------------------------------------------

use crate::theme::{REDONDEO, color, espacio, state_badge};

const ANCHO_NODO: f32 = 196.0;
const ALTO_NODO: f32 = 48.0;
const SEP_COLUMNA: f32 = 252.0;
const SEP_FILA: f32 = 64.0;

/// Dibuja el árbol y devuelve el id de sesión del nodo que se haya pulsado.
///
/// Las aristas se pintan antes que los nodos para que pasen por detrás, y son
/// curvas y no líneas rectas: una curva deja claro de dónde sale y a dónde
/// llega sin necesidad de flechas.
pub fn pintar_grafo(
    ui: &mut egui::Ui,
    grafo: &Grafo,
    seleccionado: Option<&str>,
    zoom: f32,
) -> Option<String> {
    let (ancho_nodo, alto_nodo) = (ANCHO_NODO * zoom, ALTO_NODO * zoom);
    let (sep_col, sep_fila) = (SEP_COLUMNA * zoom, SEP_FILA * zoom);

    let columnas = grafo.nodos.iter().map(|n| n.columna).max().unwrap_or(0) + 1;
    let lienzo = egui::vec2(
        columnas as f32 * sep_col + espacio::XL,
        grafo.filas_totales() * sep_fila + espacio::XL,
    );

    let (respuesta, pintor) = ui.allocate_painter(lienzo, egui::Sense::click());
    let origen = respuesta.rect.min + egui::vec2(espacio::L, espacio::L);

    let centro = |n: &Nodo| -> egui::Pos2 {
        origen
            + egui::vec2(
                n.columna as f32 * sep_col + ancho_nodo / 2.0,
                n.fila * sep_fila + alto_nodo / 2.0,
            )
    };

    let por_id: std::collections::HashMap<&str, &Nodo> =
        grafo.nodos.iter().map(|n| (n.id.as_str(), n)).collect();

    for (padre, hijo) in &grafo.aristas {
        let (Some(p), Some(h)) = (por_id.get(padre.as_str()), por_id.get(hijo.as_str())) else {
            continue;
        };

        let desde = centro(p) + egui::vec2(ancho_nodo / 2.0, 0.0);
        let hasta = centro(h) - egui::vec2(ancho_nodo / 2.0, 0.0);
        let tiron = (hasta.x - desde.x) * 0.5;

        pintor.add(egui::Shape::CubicBezier(
            egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    desde,
                    desde + egui::vec2(tiron, 0.0),
                    hasta - egui::vec2(tiron, 0.0),
                    hasta,
                ],
                false,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(1.4 * zoom, color::LINEA),
            ),
        ));
    }

    let mut pulsado = None;

    for n in &grafo.nodos {
        let rect = egui::Rect::from_center_size(centro(n), egui::vec2(ancho_nodo, alto_nodo));
        let (simbolo, color_estado) = state_badge(n.estado);
        let activo = seleccionado == Some(n.id.as_str());

        let hover = respuesta.hover_pos().is_some_and(|p| rect.contains(p));

        let fondo = if activo || hover {
            color::SUPERFICIE_ALTA
        } else {
            color::SUPERFICIE
        };
        let borde = if activo { color::ACENTO } else { color::BORDE };

        pintor.rect_filled(rect, REDONDEO * zoom, fondo);
        pintor.rect_stroke(
            rect,
            REDONDEO * zoom,
            egui::Stroke::new(if activo { 1.6 } else { 1.0 }, borde),
            egui::StrokeKind::Inside,
        );

        // Barra de acento a la izquierda: el color del estado sin teñir todo.
        let barra = egui::Rect::from_min_size(
            rect.min + egui::vec2(1.0, 1.0),
            egui::vec2(3.5 * zoom, rect.height() - 2.0),
        );
        pintor.rect_filled(barra, 2.0, color_estado);

        let x_texto = rect.min.x + espacio::M * zoom;

        pintor.text(
            egui::pos2(x_texto, rect.center().y - 8.0 * zoom),
            egui::Align2::LEFT_CENTER,
            &n.etiqueta,
            egui::FontId::proportional(13.5 * zoom),
            color::TEXTO,
        );
        pintor.text(
            egui::pos2(x_texto, rect.center().y + 9.0 * zoom),
            egui::Align2::LEFT_CENTER,
            &n.detalle,
            egui::FontId::proportional(11.5 * zoom),
            color::TEXTO_TENUE,
        );
        pintor.text(
            egui::pos2(rect.max.x - espacio::M * zoom, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            simbolo,
            egui::FontId::proportional(14.0 * zoom),
            color_estado,
        );

        if respuesta.clicked()
            && let Some(pos) = respuesta.interact_pointer_pos()
            && rect.contains(pos)
            && let Some(sid) = &n.session_id
        {
            pulsado = Some(sid.clone());
        }
    }

    pulsado
}

#[cfg(test)]
mod tests {
    use super::*;
    use argos_core::model::{ClientKind, Confidence};
    use std::path::PathBuf;

    fn fila(id: &str, rama: &str, state: AgentState, padre: Option<&str>) -> SessionRow {
        SessionRow {
            id: id.to_string(),
            client: ClientKind::ClaudeCode,
            anchor_path: PathBuf::from("/repo"),
            project: Some(PathBuf::from("/repo")),
            source_path: None,
            branch: Some(rama.to_string()),
            warp_focus_url: None,
            pid: None,
            started_at: None,
            last_activity: Utc::now(),
            state,
            confidence: Confidence::High,
            parent_id: padre.map(str::to_string),
            depth: if padre.is_some() { 1 } else { 0 },
            metrics: None,
        }
    }

    fn grafo(rows: &[SessionRow]) -> Grafo {
        construir_grafo(rows, Filter::All, Ventana::Todo, Utc::now())
    }

    fn nodo<'a>(g: &'a Grafo, id: &str) -> &'a Nodo {
        g.nodos.iter().find(|n| n.id == id).expect("nodo presente")
    }

    #[test]
    fn cada_nivel_de_la_jerarquia_va_en_su_columna() {
        let g = grafo(&[
            fila("s1", "main", AgentState::Working, None),
            fila("sub1", "main", AgentState::Working, Some("s1")),
        ]);

        assert_eq!(nodo(&g, "rama:main").columna, 0);
        assert_eq!(nodo(&g, "s1").columna, 1);
        assert_eq!(nodo(&g, "sub1").columna, 2);
    }

    #[test]
    fn las_aristas_conectan_cada_nodo_con_su_padre() {
        let g = grafo(&[
            fila("s1", "main", AgentState::Working, None),
            fila("sub1", "main", AgentState::Working, Some("s1")),
        ]);

        assert!(g.aristas.contains(&("rama:main".into(), "s1".into())));
        assert!(g.aristas.contains(&("s1".into(), "sub1".into())));
        assert_eq!(g.aristas.len(), 2);
    }

    /// Un padre centrado entre sus hijos es lo que hace legible un árbol:
    /// sin esto las líneas se cruzan y no se ve quién cuelga de quién.
    #[test]
    fn un_nodo_se_centra_verticalmente_entre_sus_hijos() {
        let g = grafo(&[
            fila("s1", "main", AgentState::Working, None),
            fila("a", "main", AgentState::Working, Some("s1")),
            fila("b", "main", AgentState::Working, Some("s1")),
        ]);

        let (fa, fb) = (nodo(&g, "a").fila, nodo(&g, "b").fila);
        assert_ne!(fa, fb, "los hermanos no se enciman");
        assert!(
            (nodo(&g, "s1").fila - (fa + fb) / 2.0).abs() < f32::EPSILON,
            "el padre va justo en medio"
        );
    }

    #[test]
    fn una_sesion_sin_subagentes_ocupa_su_propia_fila() {
        let g = grafo(&[
            fila("s1", "main", AgentState::Working, None),
            fila("s2", "main", AgentState::Working, None),
        ]);

        assert_ne!(nodo(&g, "s1").fila, nodo(&g, "s2").fila);
    }

    #[test]
    fn dentro_de_una_rama_lo_que_te_espera_va_arriba() {
        let g = grafo(&[
            fila("terminada", "main", AgentState::Finished, None),
            fila("esperando", "main", AgentState::Waiting, None),
        ]);

        assert!(
            nodo(&g, "esperando").fila < nodo(&g, "terminada").fila,
            "menor fila = más arriba"
        );
    }

    #[test]
    fn las_ramas_se_ordenan_por_urgencia_igual_que_el_resto() {
        let g = grafo(&[
            fila("a", "zzz-te-espera", AgentState::Waiting, None),
            fila("b", "aaa-terminada", AgentState::Finished, None),
        ]);

        assert!(nodo(&g, "rama:zzz-te-espera").fila < nodo(&g, "rama:aaa-terminada").fila);
    }

    /// Un nodo filtrado no puede dejar una línea colgando hacia la nada.
    #[test]
    fn filtrar_quita_el_nodo_y_tambien_sus_aristas() {
        let rows = vec![
            fila("viva", "main", AgentState::Waiting, None),
            fila("terminada", "main", AgentState::Finished, None),
        ];

        let g = construir_grafo(&rows, Filter::NeedsAttention, Ventana::Todo, Utc::now());

        assert!(g.nodos.iter().all(|n| n.id != "terminada"));
        assert!(g.aristas.iter().all(|(_, hijo)| hijo != "terminada"));
    }

    #[test]
    fn sin_sesiones_el_grafo_queda_vacio() {
        let g = grafo(&[]);
        assert!(g.nodos.is_empty());
        assert!(g.aristas.is_empty());
    }

    #[test]
    fn el_alto_del_grafo_permite_dimensionar_el_lienzo() {
        let g = grafo(&[
            fila("s1", "main", AgentState::Working, None),
            fila("s2", "otra", AgentState::Working, None),
        ]);

        assert!(g.filas_totales() >= 2.0);
    }
}
