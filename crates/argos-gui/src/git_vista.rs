use argos_core::git_history::Commit;
use chrono::{DateTime, Utc};
use std::collections::HashSet;

pub struct NodoCommit {
    pub sha: String,
    pub mensaje: String,
    pub fecha: DateTime<Utc>,
    pub refs: Vec<String>,
    pub es_merge: bool,
    /// Columna: qué línea vertical ocupa este commit.
    pub carril: usize,
    /// Fila: su posición en el orden cronológico recibido.
    pub fila: usize,
}

pub struct Arista {
    pub hijo: String,
    pub padre: String,
}

#[derive(Default)]
pub struct GrafoGit {
    pub nodos: Vec<NodoCommit>,
    pub aristas: Vec<Arista>,
    pub carriles: usize,
}

/// Reparte los commits en carriles verticales, como hace cualquier visor de
/// git. Recibe los commits en orden cronológico descendente.
///
/// La idea: cada carril "espera" el sha del siguiente commit que le toca. Un
/// commit toma el carril que lo esperaba; su primer padre hereda ese carril y
/// los demás padres abren carriles nuevos. Cuando dos carriles esperan el
/// mismo commit —un merge que se cierra— el sobrante queda libre y se
/// reutiliza, que es lo que impide que el grafo crezca a lo ancho sin parar.
pub fn tender_carriles(commits: &[Commit]) -> GrafoGit {
    let conocidos: HashSet<&str> = commits.iter().map(|c| c.sha.as_str()).collect();

    let mut esperando: Vec<Option<String>> = Vec::new();
    let mut grafo = GrafoGit::default();

    for (fila, commit) in commits.iter().enumerate() {
        // El carril que ya esperaba este commit; si ninguno, uno nuevo.
        let carril = esperando
            .iter()
            .position(|e| e.as_deref() == Some(commit.sha.as_str()))
            .unwrap_or_else(|| libre(&mut esperando));

        // Otros carriles esperando lo mismo: aquí es donde el merge cierra.
        for (i, e) in esperando.iter_mut().enumerate() {
            if i != carril && e.as_deref() == Some(commit.sha.as_str()) {
                *e = None;
            }
        }

        esperando[carril] = commit.padres.first().cloned();

        for padre in commit.padres.iter().skip(1) {
            let destino = esperando
                .iter()
                .position(|e| e.as_deref() == Some(padre.as_str()))
                .unwrap_or_else(|| libre(&mut esperando));
            esperando[destino] = Some(padre.clone());
        }

        for padre in &commit.padres {
            if conocidos.contains(padre.as_str()) {
                grafo.aristas.push(Arista {
                    hijo: commit.sha.clone(),
                    padre: padre.clone(),
                });
            }
        }

        grafo.nodos.push(NodoCommit {
            sha: commit.sha.clone(),
            mensaje: commit.mensaje.clone(),
            fecha: commit.fecha,
            refs: commit.refs.clone(),
            es_merge: commit.es_merge(),
            carril,
            fila,
        });

        grafo.carriles = grafo.carriles.max(carril + 1);
    }

    grafo
}

/// Nombres de rama presentables: `main` y `origin/main` son la misma rama
/// para quien mira, así que se colapsan en una etiqueta. `HEAD` y las
/// etiquetas de versión no son ramas.
pub fn nombres_de_rama(refs: &[String]) -> Vec<String> {
    let mut vistos = Vec::new();

    for r in refs {
        if r.starts_with("tag: ") {
            continue;
        }
        let nombre = r.strip_prefix("origin/").unwrap_or(r).trim();
        if nombre.is_empty() || nombre == "HEAD" {
            continue;
        }
        if !vistos.iter().any(|v| v == nombre) {
            vistos.push(nombre.to_string());
        }
    }

    vistos
}

/// El carril libre más a la izquierda, creando uno si no hay.
fn libre(esperando: &mut Vec<Option<String>>) -> usize {
    match esperando.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            esperando.push(None);
            esperando.len() - 1
        }
    }
}

// --- pintado ---------------------------------------------------------------

use crate::theme::{
    REDONDEO, color, color_de_carril, edad_legible, espacio, inicial_de_cliente, state_badge,
};
use std::collections::HashMap;

// Dimensionados para que quepa la insignia de un agente dentro del nodo
// sin que los puntos se toquen.
const SEP_COMMIT: f32 = 46.0;
const SEP_CARRIL: f32 = 62.0;
const RADIO: f32 = 11.0;

/// Agentes presentes ahora mismo en las ramas que apuntan a este commit.
///
/// Solo los activos: un agente que terminó ya no está ahí, y marcarlo daría
/// la impresión de actividad donde no la hay.
pub fn agentes_en_punta<'a>(
    refs: &[String],
    filas: &'a [argos_core::store::SessionRow],
) -> Vec<&'a argos_core::store::SessionRow> {
    use argos_core::model::AgentState;

    let ramas = nombres_de_rama(refs);
    if ramas.is_empty() {
        return Vec::new();
    }

    filas
        .iter()
        .filter(|f| matches!(f.state, AgentState::Working | AgentState::Waiting))
        .filter(|f| {
            f.branch
                .as_ref()
                .is_some_and(|b| ramas.iter().any(|r| r == b))
        })
        .collect()
}

/// Ancho que ocupará el grafo, para poder abrir la vista por su extremo
/// derecho sin recurrir a un desplazamiento infinito.
pub fn ancho_estimado(grafo: &GrafoGit, zoom: f32) -> f32 {
    grafo.nodos.len() as f32 * SEP_COMMIT * zoom + espacio::XL * 4.0
}

/// Dibuja la historia de izquierda a derecha: el tiempo avanza hacia la
/// derecha y las ramas son carriles horizontales.
///
/// **Solo el árbol.** El texto de cada commit se revela al acercar el cursor
/// y el detalle completo al pulsar: una pared de mensajes tapa la forma del
/// árbol, que es lo que se viene a ver.
///
/// Devuelve el sha pulsado, si lo hubo.
pub struct Pintura<'a> {
    pub estado_ramas: &'a HashMap<String, (u32, u32)>,
    pub filas: &'a [argos_core::store::SessionRow],
    pub logos: &'a mut crate::logos::Logos,
    pub seleccionado: Option<&'a str>,
    pub now: DateTime<Utc>,
    pub zoom: f32,
}

pub fn pintar_git(ui: &mut egui::Ui, grafo: &GrafoGit, p: &mut Pintura<'_>) -> Option<String> {
    let Pintura {
        estado_ramas,
        filas,
        logos,
        seleccionado,
        now,
        zoom,
    } = p;
    let (now, zoom, seleccionado) = (*now, *zoom, *seleccionado);
    // Reloj continuo de egui: mueve la animación sin depender de la hora.
    let t = ui.input(|i| i.time) as f32;
    let sep_commit = SEP_COMMIT * zoom;
    let sep_carril = SEP_CARRIL * zoom;
    let radio = RADIO * zoom;

    // El más antiguo a la izquierda: se lee como se lee, hacia adelante.
    let ultimo = grafo.nodos.len().saturating_sub(1);

    // Las etiquetas de rama se dibujan encima de su commit, así que el
    // primer carril necesita sitio o quedan recortadas contra el borde.
    let margen_arriba = espacio::XL * 2.0;

    let lienzo = egui::vec2(
        grafo.nodos.len() as f32 * sep_commit + espacio::XL * 4.0,
        (grafo.carriles as f32 * sep_carril + margen_arriba + espacio::XL).max(200.0),
    );
    let (respuesta, pintor) = ui.allocate_painter(lienzo, egui::Sense::click());
    let origen = respuesta.rect.min + egui::vec2(espacio::XL, margen_arriba);

    let punto = |n: &NodoCommit| -> egui::Pos2 {
        origen
            + egui::vec2(
                (ultimo - n.fila) as f32 * sep_commit,
                n.carril as f32 * sep_carril,
            )
    };

    let por_sha: HashMap<&str, &NodoCommit> =
        grafo.nodos.iter().map(|n| (n.sha.as_str(), n)).collect();

    for a in &grafo.aristas {
        let (Some(hijo), Some(padre)) =
            (por_sha.get(a.hijo.as_str()), por_sha.get(a.padre.as_str()))
        else {
            continue;
        };

        let (desde, hasta) = (punto(padre), punto(hijo));
        let trazo = egui::Stroke::new(1.6 * zoom, color_de_carril(padre.carril));

        if (desde.y - hasta.y).abs() < f32::EPSILON {
            pintor.line_segment([desde, hasta], trazo);
        } else {
            let medio = (desde.x + hasta.x) / 2.0;
            pintor.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        desde,
                        egui::pos2(medio, desde.y),
                        egui::pos2(medio, hasta.y),
                        hasta,
                    ],
                    false,
                    egui::Color32::TRANSPARENT,
                    trazo,
                ),
            ));
        }
    }

    let cursor = respuesta.hover_pos();
    let mut pulsado = None;
    let mut bajo_el_cursor: Option<&NodoCommit> = None;

    for n in &grafo.nodos {
        let c = punto(n);
        let col = color_de_carril(n.carril);
        let cerca = cursor.is_some_and(|p| (p - c).length() < sep_commit * 0.6);
        let activo = seleccionado == Some(n.sha.as_str());

        if cerca {
            bajo_el_cursor = Some(n);
        }

        let r = if cerca || activo { radio * 1.5 } else { radio };

        // Un merge va hueco para distinguirlo de un commit normal.
        if n.es_merge {
            pintor.circle_filled(c, r, color::FONDO);
            pintor.circle_stroke(c, r, egui::Stroke::new(2.0 * zoom, col));
        } else {
            pintor.circle_filled(c, r, col);
        }

        if activo {
            pintor.circle_stroke(c, r + 4.0 * zoom, egui::Stroke::new(1.5, color::TEXTO));
        }

        // Si un agente trabaja ahora en la punta de esta rama, su insignia va
        // dentro del nodo: es lo que une este árbol con la vista de agentes.
        let agentes = agentes_en_punta(&n.refs, filas);

        for (i, agente) in agentes.iter().enumerate() {
            let (_, color_estado) = state_badge(agente.state);
            let radio_insignia = r * 0.92;

            // Flota al lado del nodo, con un desfase por agente para que dos
            // no se muevan al unísono, que se vería mecánico.
            let fase = t * 1.9 + i as f32 * 1.3;
            let flote = egui::vec2(
                (r + radio_insignia + 6.0 * zoom) + i as f32 * radio_insignia * 2.3,
                fase.sin() * 5.0 * zoom,
            );
            let centro_insignia = c + flote;

            // Latido suave solo si está trabajando: al esperar, quieto.
            let latido = if agente.state == argos_core::model::AgentState::Working {
                1.0 + (t * 2.6).sin() * 0.07
            } else {
                1.0
            };
            let radio_insignia = radio_insignia * latido;

            pintor.circle_filled(centro_insignia, radio_insignia, color::SUPERFICIE);
            pintor.circle_stroke(
                centro_insignia,
                radio_insignia,
                egui::Stroke::new(2.0 * zoom, color_estado),
            );

            match logos.textura(ui.ctx(), agente.client) {
                Some(tex) => {
                    let lado = radio_insignia * 1.25;
                    pintor.image(
                        tex.id(),
                        egui::Rect::from_center_size(centro_insignia, egui::vec2(lado, lado)),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                None => {
                    pintor.text(
                        centro_insignia,
                        egui::Align2::CENTER_CENTER,
                        inicial_de_cliente(agente.client),
                        egui::FontId::proportional(11.5 * zoom),
                        color_estado,
                    );
                }
            }

            // Un hilo fino hasta el nodo: deja claro de quién cuelga.
            pintor.line_segment(
                [
                    c + egui::vec2(r, 0.0),
                    centro_insignia - egui::vec2(radio_insignia, 0.0),
                ],
                egui::Stroke::new(1.0 * zoom, color_estado.gamma_multiply(0.5)),
            );
        }

        // Las puntas de rama sí llevan etiqueta siempre: son lo que orienta.
        for (i, nombre) in nombres_de_rama(&n.refs).iter().enumerate() {
            let estado = estado_ramas.get(nombre.as_str());
            let etiqueta = match estado {
                Some((a, b)) if *a > 0 || *b > 0 => format!("{nombre} ↑{a} ↓{b}"),
                _ => nombre.clone(),
            };

            let galera = pintor.layout_no_wrap(
                etiqueta,
                egui::FontId::proportional(10.0 * zoom),
                color::TEXTO,
            );
            let caja = egui::Rect::from_min_size(
                egui::pos2(
                    c.x - galera.size().x / 2.0,
                    c.y - radio - 12.0 * zoom - i as f32 * 17.0 * zoom - galera.size().y,
                ),
                galera.size() + egui::vec2(espacio::S, 3.0),
            );
            pintor.rect_filled(caja, REDONDEO * 0.5, color::SUPERFICIE_ALTA);
            pintor.galley(
                caja.min + egui::vec2(espacio::S / 2.0, 1.5),
                galera,
                color::TEXTO,
            );
        }

        if respuesta.clicked() && cerca {
            pulsado = Some(n.sha.clone());
        }
    }

    // Al acercar el cursor: lo justo para identificar el commit sin taparlo.
    if let Some(n) = bajo_el_cursor {
        egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            egui::Id::new("commit-tooltip"),
            egui::PopupAnchor::Pointer,
        )
        .show(|ui| {
            ui.horizontal(|ui| {
                ui.monospace(&n.sha);
                ui.weak(edad_legible((now - n.fecha).num_seconds()));
            });
            ui.label(&n.mensaje);
        });
    }

    pulsado
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// Construye commits en el orden en que los entrega `git log --date-order`:
    /// del más reciente al más antiguo.
    fn commit(sha: &str, padres: &[&str]) -> Commit {
        Commit {
            sha: sha.to_string(),
            mensaje: format!("mensaje de {sha}"),
            autor: "Alex".into(),
            fecha: Utc::now() - Duration::minutes(1),
            padres: padres.iter().map(|p| p.to_string()).collect(),
            refs: vec![],
        }
    }

    fn carril_de(g: &GrafoGit, sha: &str) -> usize {
        g.nodos
            .iter()
            .find(|n| n.sha == sha)
            .expect("el commit está en el grafo")
            .carril
    }

    #[test]
    fn una_historia_lineal_ocupa_un_solo_carril() {
        let g = tender_carriles(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);

        assert_eq!(g.carriles, 1);
        assert!(g.nodos.iter().all(|n| n.carril == 0));
    }

    /// El caso que justifica todo el algoritmo: una rama que diverge ocupa su
    /// propio carril hasta que el merge la vuelve a juntar.
    #[test]
    fn una_rama_que_diverge_abre_un_carril_propio() {
        // m une main (b) con la rama (r); ambas salen de "base".
        let g = tender_carriles(&[
            commit("m", &["b", "r"]),
            commit("b", &["base"]),
            commit("r", &["base"]),
            commit("base", &[]),
        ]);

        assert_eq!(carril_de(&g, "m"), 0);
        assert_eq!(carril_de(&g, "b"), 0, "el primer padre sigue el carril");
        assert_eq!(carril_de(&g, "r"), 1, "el segundo padre abre otro");
        assert_eq!(g.carriles, 2);
    }

    /// Al cerrarse una rama, su carril queda libre y debe reutilizarse en vez
    /// de que el grafo crezca a lo ancho sin parar.
    #[test]
    fn el_carril_de_una_rama_cerrada_se_reutiliza() {
        let g = tender_carriles(&[
            commit("m2", &["x", "y"]),
            commit("x", &["m1"]),
            commit("y", &["m1"]),
            commit("m1", &["b", "r"]),
            commit("b", &["base"]),
            commit("r", &["base"]),
            commit("base", &[]),
        ]);

        assert_eq!(g.carriles, 2, "dos ramas nunca simultáneas caben en dos");
    }

    #[test]
    fn cada_commit_se_une_con_todos_sus_padres() {
        let g = tender_carriles(&[
            commit("m", &["p1", "p2"]),
            commit("p1", &[]),
            commit("p2", &[]),
        ]);

        assert!(g.aristas.iter().any(|a| a.hijo == "m" && a.padre == "p1"));
        assert!(g.aristas.iter().any(|a| a.hijo == "m" && a.padre == "p2"));
        assert_eq!(g.aristas.len(), 2);
    }

    /// Un padre fuera del límite de commits cargados no debe dejar una arista
    /// apuntando a la nada.
    #[test]
    fn una_arista_a_un_padre_no_cargado_se_descarta() {
        let g = tender_carriles(&[commit("c", &["fuera-del-limite"])]);

        assert!(g.aristas.is_empty(), "el padre no está en el grafo");
        assert_eq!(g.nodos.len(), 1);
    }

    #[test]
    fn las_filas_siguen_el_orden_recibido() {
        let g = tender_carriles(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);

        assert_eq!(g.nodos[0].fila, 0);
        assert_eq!(g.nodos[1].fila, 1);
        assert_eq!(g.nodos[2].fila, 2);
    }

    #[test]
    fn sin_commits_el_grafo_queda_vacio() {
        let g = tender_carriles(&[]);
        assert!(g.nodos.is_empty());
        assert_eq!(g.carriles, 0);
    }

    /// `main`, `origin/main` y `origin/HEAD` son lo mismo para quien mira:
    /// mostrarlos como tres etiquetas es ruido.
    #[test]
    fn las_etiquetas_de_rama_no_se_duplican_por_el_remoto() {
        let refs = vec![
            "main".to_string(),
            "origin/main".to_string(),
            "origin/HEAD".to_string(),
        ];

        assert_eq!(nombres_de_rama(&refs), vec!["main"]);
    }

    #[test]
    fn una_etiqueta_de_version_no_es_una_rama() {
        let refs = vec!["tag: v1.0".to_string(), "feat/x".to_string()];
        assert_eq!(nombres_de_rama(&refs), vec!["feat/x"]);
    }

    /// La punta de una rama es donde un agente está trabajando ahora: es lo
    /// que une el árbol de git con la vista de agentes.
    #[test]
    fn un_commit_en_la_punta_de_una_rama_recoge_a_sus_agentes_activos() {
        use argos_core::model::{AgentState, ClientKind, Confidence};
        use std::path::PathBuf;

        let fila = |rama: &str, estado: AgentState| argos_core::store::SessionRow {
            id: format!("{rama}-{estado:?}"),
            client: ClientKind::ClaudeCode,
            anchor_path: PathBuf::from("/repo"),
            project: None,
            source_path: None,
            branch: Some(rama.to_string()),
            warp_focus_url: None,
            pid: None,
            started_at: None,
            last_activity: Utc::now(),
            state: estado,
            confidence: Confidence::High,
            parent_id: None,
            depth: 0,
            metrics: None,
        };

        let filas = vec![
            fila("main", AgentState::Working),
            fila("main", AgentState::Finished),
            fila("otra", AgentState::Waiting),
        ];

        let refs = vec!["main".to_string()];
        let activos = agentes_en_punta(&refs, &filas);

        assert_eq!(activos.len(), 1, "solo el que está trabajando");
        assert_eq!(activos[0].state, AgentState::Working);
    }

    #[test]
    fn un_agente_esperando_respuesta_tambien_cuenta_como_presente() {
        use argos_core::model::{AgentState, ClientKind, Confidence};
        use std::path::PathBuf;

        let filas = vec![argos_core::store::SessionRow {
            id: "s".into(),
            client: ClientKind::Codex,
            anchor_path: PathBuf::from("/repo"),
            project: None,
            source_path: None,
            branch: Some("main".into()),
            warp_focus_url: None,
            pid: None,
            started_at: None,
            last_activity: Utc::now(),
            state: AgentState::Waiting,
            confidence: Confidence::High,
            parent_id: None,
            depth: 0,
            metrics: None,
        }];

        assert_eq!(agentes_en_punta(&["main".to_string()], &filas).len(), 1);
    }

    #[test]
    fn un_commit_sin_etiqueta_de_rama_no_tiene_agentes() {
        assert!(agentes_en_punta(&[], &[]).is_empty());
    }

    #[test]
    fn un_merge_se_marca_como_tal() {
        let g = tender_carriles(&[commit("m", &["a", "b"]), commit("a", &[]), commit("b", &[])]);

        assert!(g.nodos.iter().find(|n| n.sha == "m").unwrap().es_merge);
        assert!(!g.nodos.iter().find(|n| n.sha == "a").unwrap().es_merge);
    }
}
