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

use crate::theme::{REDONDEO, color, color_de_carril, edad_legible, espacio};
use std::collections::HashMap;

const SEP_CARRIL: f32 = 20.0;
const ALTO_FILA: f32 = 30.0;
const RADIO: f32 = 5.0;
const ANCHO_CARRILES: f32 = 150.0;

/// Dibuja la historia: el tiempo baja y las ramas son carriles paralelos.
/// Es la convención de cualquier visor de git, y funciona porque una historia
/// larga se lee mejor en vertical.
pub fn pintar_git(
    ui: &mut egui::Ui,
    grafo: &GrafoGit,
    estado_ramas: &HashMap<String, (u32, u32)>,
    now: DateTime<Utc>,
    zoom: f32,
) {
    let alto_fila = ALTO_FILA * zoom;
    let sep_carril = SEP_CARRIL * zoom;
    let ancho_carriles = (ANCHO_CARRILES * zoom).min(sep_carril * grafo.carriles as f32 + 40.0);

    let lienzo = egui::vec2(
        ui.available_width().max(600.0),
        grafo.nodos.len() as f32 * alto_fila + espacio::XL,
    );
    let (respuesta, pintor) = ui.allocate_painter(lienzo, egui::Sense::hover());
    let origen = respuesta.rect.min + egui::vec2(espacio::L, espacio::L);

    let punto = |n: &NodoCommit| -> egui::Pos2 {
        origen
            + egui::vec2(
                n.carril as f32 * sep_carril + RADIO * zoom,
                n.fila as f32 * alto_fila,
            )
    };

    let por_sha: HashMap<&str, &NodoCommit> =
        grafo.nodos.iter().map(|n| (n.sha.as_str(), n)).collect();

    // Las líneas primero, para que pasen por detrás de los puntos.
    for a in &grafo.aristas {
        let (Some(hijo), Some(padre)) =
            (por_sha.get(a.hijo.as_str()), por_sha.get(a.padre.as_str()))
        else {
            continue;
        };

        let (desde, hasta) = (punto(hijo), punto(padre));
        // El color lo pone el carril de destino: así una rama conserva el
        // suyo al bajar y el merge se ve entrar en el carril principal.
        let trazo = egui::Stroke::new(1.6 * zoom, color_de_carril(padre.carril));

        if (desde.x - hasta.x).abs() < f32::EPSILON {
            pintor.line_segment([desde, hasta], trazo);
        } else {
            let medio = (desde.y + hasta.y) / 2.0;
            pintor.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        desde,
                        egui::pos2(desde.x, medio),
                        egui::pos2(hasta.x, medio),
                        hasta,
                    ],
                    false,
                    egui::Color32::TRANSPARENT,
                    trazo,
                ),
            ));
        }
    }

    for n in &grafo.nodos {
        let c = punto(n);
        let col = color_de_carril(n.carril);

        // Un merge se dibuja hueco para distinguirlo de un commit normal.
        if n.es_merge {
            pintor.circle_stroke(c, RADIO * zoom, egui::Stroke::new(2.0 * zoom, col));
        } else {
            pintor.circle_filled(c, RADIO * zoom, col);
        }

        let mut x = origen.x + ancho_carriles;

        for texto in nombres_de_rama(&n.refs) {
            let estado = estado_ramas.get(texto.as_str());
            let etiqueta = match estado {
                Some((a, b)) if *a > 0 || *b > 0 => format!("{texto}  ↑{a} ↓{b}"),
                _ => texto.clone(),
            };

            let galera = pintor.layout_no_wrap(
                etiqueta,
                egui::FontId::proportional(10.5 * zoom),
                color::TEXTO,
            );
            let caja = egui::Rect::from_min_size(
                egui::pos2(x, c.y - galera.size().y / 2.0 - 2.0),
                galera.size() + egui::vec2(espacio::S, 4.0),
            );
            pintor.rect_filled(caja, REDONDEO * 0.5 * zoom, color::SUPERFICIE_ALTA);
            pintor.galley(
                caja.min + egui::vec2(espacio::S / 2.0, 2.0),
                galera,
                color::TEXTO,
            );
            x = caja.max.x + espacio::S;
        }

        pintor.text(
            egui::pos2(x, c.y),
            egui::Align2::LEFT_CENTER,
            &n.sha,
            egui::FontId::monospace(11.0 * zoom),
            color::TEXTO_TENUE,
        );

        pintor.text(
            egui::pos2(x + 70.0 * zoom, c.y),
            egui::Align2::LEFT_CENTER,
            &n.mensaje,
            egui::FontId::proportional(12.5 * zoom),
            color::TEXTO,
        );

        pintor.text(
            egui::pos2(respuesta.rect.max.x - espacio::L, c.y),
            egui::Align2::RIGHT_CENTER,
            edad_legible((now - n.fecha).num_seconds()),
            egui::FontId::proportional(11.0 * zoom),
            color::TEXTO_TENUE,
        );
    }
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

    #[test]
    fn un_merge_se_marca_como_tal() {
        let g = tender_carriles(&[commit("m", &["a", "b"]), commit("a", &[]), commit("b", &[])]);

        assert!(g.nodos.iter().find(|n| n.sha == "m").unwrap().es_merge);
        assert!(!g.nodos.iter().find(|n| n.sha == "a").unwrap().es_merge);
    }
}
