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

/// Nombres de rama presentables. `HEAD` y las etiquetas de versión no son
/// ramas.
///
/// El remoto **no** se hace pasar por la rama local. Cuando los dos apuntan
/// al mismo commit sobra decirlo dos veces y se queda solo el local; pero si
/// la local va por delante caen en commits distintos, y renombrar
/// `origin/main` a `main` etiquetaba los dos igual. Como los agentes se
/// buscan por nombre de rama, los mismos acababan dibujados en ambos: un
/// agente parecía dos.
pub fn nombres_de_rama(refs: &[String]) -> Vec<String> {
    let locales: HashSet<&str> = refs
        .iter()
        .map(|r| r.trim())
        .filter(|r| !r.starts_with("tag: ") && !r.starts_with("origin/"))
        .collect();

    let mut vistos: Vec<String> = Vec::new();

    for r in refs {
        let nombre = r.trim();
        if nombre.is_empty() || nombre == "HEAD" || nombre.starts_with("tag: ") {
            continue;
        }
        // `origin/HEAD` es un puntero al remoto por defecto, no una rama.
        if nombre == "origin/HEAD" {
            continue;
        }
        // El remoto sobra solo si su local está en este mismo commit.
        if let Some(sin_remoto) = nombre.strip_prefix("origin/")
            && locales.contains(sin_remoto)
        {
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
/// Las etiquetas de rama se dibujan encima de su commit, así que el primer
/// carril necesita sitio o quedan recortadas contra el borde.
const MARGEN_ARRIBA: f32 = espacio::XL * 2.0;

const SEP_COMMIT: f32 = 76.0;
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

/// Dónde va cada insignia, en desplazamiento horizontal desde el commit.
pub struct Reparto {
    /// Un desplazamiento por agente raíz.
    pub raices: Vec<f32>,
    /// Los desplazamientos de los subagentes de cada raíz, en el mismo orden.
    pub subagentes: Vec<Vec<f32>>,
}

/// Reparte el ancho de la banda entre los agentes raíz y sus subagentes.
///
/// Separado del pintado porque es aritmética, y la aritmética de una
/// disposición se comprueba sin abrir una ventana. La regla: cada raíz
/// reserva el ancho que ocupan sus hijos —o el paso mínimo si no tiene— y
/// el conjunto se centra bajo el commit. Sin reservar por subárbol, los
/// hijos de una raíz se meten debajo de la vecina y deja de saberse de quién
/// cuelga cada uno.
pub fn repartir(subs_por_raiz: &[usize], paso_raiz: f32, paso_sub: f32) -> Reparto {
    let anchos: Vec<f32> = subs_por_raiz
        .iter()
        .map(|n| (*n as f32 * paso_sub).max(paso_raiz))
        .collect();

    let total: f32 = anchos.iter().sum();
    let mut borde = -total / 2.0;

    let mut raices = Vec::with_capacity(anchos.len());
    let mut subagentes = Vec::with_capacity(anchos.len());

    for (ancho, n) in anchos.iter().zip(subs_por_raiz) {
        let centro = borde + ancho / 2.0;
        raices.push(centro);

        // Los hijos se centran bajo su padre, con el del medio a plomo.
        let extremo = (*n as f32 - 1.0) / 2.0;
        subagentes.push(
            (0..*n)
                .map(|i| centro + (i as f32 - extremo) * paso_sub)
                .collect(),
        );

        borde += ancho;
    }

    Reparto { raices, subagentes }
}

/// Ancho que ocupará el grafo, para poder abrir la vista por su extremo
/// derecho sin recurrir a un desplazamiento infinito.
pub fn ancho_estimado(grafo: &GrafoGit, zoom: f32) -> f32 {
    grafo.nodos.len() as f32 * SEP_COMMIT * zoom + espacio::XL * 4.0
}

/// Espacio que el árbol necesita de verdad: los carriles, más sitio arriba
/// para las etiquetas de rama y abajo para la banda de agentes.
pub fn alto_de_contenido(carriles: usize, zoom: f32) -> f32 {
    (carriles.saturating_sub(1)) as f32 * SEP_CARRIL * zoom + MARGEN_ARRIBA + espacio::XL * 3.0
}

/// Alto del lienzo y cuánto bajar el árbol dentro de él.
///
/// Separado del pintado porque es la regla que se rompía sin que nadie se
/// diera cuenta: el lienzo llena la ventana —si no, al agrandarla queda un
/// hueco muerto y la barra horizontal se pega al contenido— y el sobrante se
/// reparte arriba y abajo en vez de caer todo debajo.
pub fn alto_y_centrado(contenido: f32, disponible: f32) -> (f32, f32) {
    let alto = contenido.max(disponible);
    (alto, (alto - contenido) / 2.0)
}

/// Dibuja la historia de izquierda a derecha: el tiempo avanza hacia la
/// derecha y las ramas son carriles horizontales.
///
/// **Solo el árbol.** El texto de cada commit se revela al acercar el cursor
/// y el detalle completo al pulsar: una pared de mensajes tapa la forma del
/// árbol, que es lo que se viene a ver.
///
/// Devuelve el sha pulsado, si lo hubo.
/// Qué se pulsó en el árbol. Distinguirlo importa: un commit abre su
/// detalle, un castor lleva a la terminal de ese agente.
pub enum Pulsado {
    Commit(String),
    Agente(String),
}

pub struct Pintura<'a> {
    pub estado_ramas: &'a HashMap<String, (u32, u32)>,
    pub filas: &'a [argos_core::store::SessionRow],
    pub logos: &'a mut crate::logos::Logos,
    pub mascota: &'a mut crate::mascota::Mascota,
    pub seleccionado: Option<&'a str>,
    pub now: DateTime<Utc>,
    pub zoom: f32,
    /// Alto útil de la ventana, medido **antes** de entrar al `ScrollArea`:
    /// dentro, `available_height` ya no es el de la ventana.
    pub alto_disponible: f32,
}

pub fn pintar_git(ui: &mut egui::Ui, grafo: &GrafoGit, p: &mut Pintura<'_>) -> Option<Pulsado> {
    let Pintura {
        estado_ramas,
        filas,
        logos,
        mascota,
        seleccionado,
        now,
        zoom,
        alto_disponible,
    } = p;
    let (now, zoom, seleccionado, alto_disponible) = (*now, *zoom, *seleccionado, *alto_disponible);
    // Reloj continuo de egui: mueve la animación sin depender de la hora.
    let t = ui.input(|i| i.time) as f32;
    let sep_commit = SEP_COMMIT * zoom;
    let sep_carril = SEP_CARRIL * zoom;
    let radio = RADIO * zoom;

    // El más antiguo a la izquierda: se lee como se lee, hacia adelante.
    let ultimo = grafo.nodos.len().saturating_sub(1);

    // El alto útil llega de fuera del `ScrollArea`: dentro, `available_height`
    // no es el de la ventana, y el árbol acababa pegado arriba con media
    // pantalla vacía debajo.
    let alto_contenido = alto_de_contenido(grafo.carriles, zoom);
    let (alto, centrado) = alto_y_centrado(alto_contenido, alto_disponible - espacio::S);

    let lienzo = egui::vec2(
        grafo.nodos.len() as f32 * sep_commit + espacio::XL * 4.0,
        alto,
    );
    let (respuesta, pintor) = ui.allocate_painter(lienzo, egui::Sense::click());

    let origen = respuesta.rect.min + egui::vec2(espacio::XL, MARGEN_ARRIBA + centrado);

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
    let mut agente_bajo_el_cursor: Option<&argos_core::store::SessionRow> = None;

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

        // Las insignias van en una banda bajo la línea de commits: las
        // etiquetas de rama ya ocupan arriba, y darles banda propia evita
        // confundirlas con los nodos. Ancladas a la altura de su commit.
        let radio_insignia = r * 0.82;
        // 0.72 y no menos: por debajo de ahí el sprite deja de leerse y el
        // castor pequeño se vuelve una mancha marrón.
        let radio_sub = radio_insignia * 0.72;
        // El castor mide `radio * 4.2` de alto y casi otro tanto de ancho,
        // así que las separaciones salen de su tamaño dibujado y no del radio
        // pelado: con el radio a secas se solapaban entre ellos.
        let paso = radio_insignia * 4.4;
        let paso_sub = radio_sub * 4.4;

        // Un subagente cuelga de su padre, no del commit: no trabaja en la
        // rama por su cuenta, trabaja para alguien. Y si su padre no está a
        // la vista se trata como raíz, porque colgarlo de nadie lo dejaría
        // flotando sin explicar de dónde sale.
        let presentes: HashSet<&str> = agentes.iter().map(|a| a.id.as_str()).collect();
        let raices: Vec<&argos_core::store::SessionRow> = agentes
            .iter()
            .copied()
            .filter(|a| {
                a.parent_id
                    .as_deref()
                    .is_none_or(|padre| !presentes.contains(padre))
            })
            .collect();
        let hijos: Vec<Vec<&argos_core::store::SessionRow>> = raices
            .iter()
            .map(|raiz| {
                agentes
                    .iter()
                    .copied()
                    .filter(|a| a.parent_id.as_deref() == Some(raiz.id.as_str()))
                    .collect()
            })
            .collect();

        let cuentas: Vec<usize> = hijos.iter().map(Vec::len).collect();
        let reparto = repartir(&cuentas, paso, paso_sub);

        let mut plan: Vec<(&argos_core::store::SessionRow, f32, f32, f32, bool)> = Vec::new();
        for (k, raiz) in raices.iter().enumerate() {
            plan.push((raiz, reparto.raices[k], 0.0, radio_insignia, false));
            for (j, hijo) in hijos[k].iter().enumerate() {
                // Medio castor del padre más medio del hijo, y un respiro.
                let dy = (radio_insignia + radio_sub) * 2.1 + espacio::S * zoom;
                plan.push((hijo, reparto.subagentes[k][j], dy, radio_sub, true));
            }
        }

        for (i, (agente, dx, dy, radio_base, es_subagente)) in plan.iter().copied().enumerate() {
            let (simbolo_estado, color_estado) = state_badge(agente.state);
            let trabajando = agente.state == argos_core::model::AgentState::Working;

            // Un vaivén mínimo alrededor de su sitio: da señal de vida sin
            // desalinearlas. El desfase evita que se muevan al unísono.
            let fase = t * 1.8 + i as f32 * 1.3;
            let vaiven = if trabajando {
                fase.sin() * 2.0 * zoom
            } else {
                0.0
            };

            let base_y = c.y + r + radio_insignia + 30.0 * zoom;
            let centro_insignia = egui::pos2(c.x + dx, base_y + dy + vaiven);

            // De dónde sale el hilo: del commit si es raíz, del padre si no.
            let ancla = if es_subagente {
                egui::pos2(
                    c.x + reparto.raices[raices
                        .iter()
                        .position(|raiz| Some(raiz.id.as_str()) == agente.parent_id.as_deref())
                        .unwrap_or(0)],
                    // De los pies del padre, no de su centro.
                    base_y + radio_insignia * 1.34,
                )
            } else {
                egui::pos2(c.x, c.y + r)
            };

            // Late solo si trabaja: al esperarte, quieto. El movimiento
            // significa actividad y no debe mentir.
            let radio_latido = if trabajando {
                radio_base * (1.0 + (t * 2.6).sin() * 0.07)
            } else {
                radio_base
            };

            // El castor es pulsable: lleva a la terminal de ese agente.
            let radio_toque = radio_base * 2.2;
            let sobre_el_castor =
                cursor.is_some_and(|q| (q - centro_insignia).length() < radio_toque);

            if sobre_el_castor {
                agente_bajo_el_cursor = Some(agente);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

                if respuesta.clicked() {
                    pulsado = Some(Pulsado::Agente(agente.id.clone()));
                }
            }

            // Hilo hasta su ancla: deja claro de quién cuelga.
            pintor.line_segment(
                [
                    ancla,
                    egui::pos2(centro_insignia.x, centro_insignia.y - radio_latido),
                ],
                egui::Stroke::new(1.0 * zoom, color_estado.gamma_multiply(0.45)),
            );

            // Con la mascota instalada, ella lleva el estado: tiene una
            // animación propia por cada uno. El logo de plataforma queda
            // debajo, pequeño, para saber quién es sin repetir información.
            if let Some((tex, columnas, filas_atlas)) = mascota.textura(ui.ctx()) {
                let tira = crate::mascota::tira_de(agente.state);
                let fotograma = crate::mascota::fotograma_en(tira, t);
                let uv = crate::mascota::uv_de(tira, fotograma, columnas, filas_atlas);

                let alto = radio_latido * 4.2;
                let ancho = alto * 192.0 / 208.0;

                // Un suelo del color del estado bajo los pies. Sin él, el
                // estado solo lo diría la animación, y una animación no se
                // lee de un vistazo ni sirve a quien no distingue los tonos.
                pintor.add(egui::Shape::Ellipse(egui::epaint::EllipseShape::filled(
                    centro_insignia + egui::vec2(0.0, alto * 0.30),
                    egui::vec2(ancho * 0.46, ancho * 0.15),
                    color_estado.gamma_multiply(0.30),
                )));
                pintor.add(egui::Shape::Ellipse(egui::epaint::EllipseShape::stroke(
                    centro_insignia + egui::vec2(0.0, alto * 0.30),
                    egui::vec2(ancho * 0.46, ancho * 0.15),
                    egui::Stroke::new(1.4 * zoom, color_estado),
                )));

                pintor.image(
                    tex,
                    egui::Rect::from_center_size(
                        centro_insignia - egui::vec2(0.0, alto * 0.18),
                        egui::vec2(ancho, alto),
                    ),
                    uv,
                    egui::Color32::WHITE,
                );

                // Símbolo del estado junto a la inicial de la plataforma: el
                // color solo refuerza, nunca es la única señal (spec §8).
                pintor.text(
                    centro_insignia + egui::vec2(0.0, alto * 0.48),
                    egui::Align2::CENTER_CENTER,
                    format!("{simbolo_estado} {}", inicial_de_cliente(agente.client)),
                    egui::FontId::proportional(10.5 * zoom),
                    color_estado,
                );

                continue;
            }

            pintor.circle_filled(centro_insignia, radio_latido, color::SUPERFICIE);
            pintor.circle_stroke(
                centro_insignia,
                radio_latido,
                egui::Stroke::new(2.0 * zoom, color_estado),
            );

            match logos.textura(ui.ctx(), agente.client) {
                Some(tex) => {
                    let lado = radio_latido * 1.25;
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
                        egui::FontId::proportional(11.0 * zoom),
                        color_estado,
                    );
                }
            }
        }

        // Las puntas de rama sí llevan etiqueta siempre: son lo que orienta.
        for (i, nombre) in nombres_de_rama(&n.refs).iter().enumerate() {
            let etiqueta = match estado_ramas.get(nombre.as_str()) {
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

        // El castor gana sobre el commit: está encima y es más pequeño.
        if respuesta.clicked() && cerca && pulsado.is_none() {
            pulsado = Some(Pulsado::Commit(n.sha.clone()));
        }
    }

    // El globo del agente tiene prioridad: si el cursor está sobre un castor,
    // lo que interesa es él y no el commit del que cuelga.
    if let Some(a) = agente_bajo_el_cursor {
        egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            egui::Id::new("agente-tooltip"),
            egui::PopupAnchor::Pointer,
        )
        .show(|ui| {
            ui.horizontal(|ui| {
                let (simbolo, col) = state_badge(a.state);
                ui.colored_label(col, simbolo);
                ui.strong(a.client.label());
            });
            ui.label(crate::theme::state_label(a.state));
            match a.warp_focus_url {
                Some(_) => ui.weak("clic para ir a su terminal"),
                None => ui.weak("sin pane de Warp asociada"),
            };
        });
    } else if let Some(n) = bajo_el_cursor {
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
            coautores: vec![],
            refs: vec![],
        }
    }

    /// Con la ventana alta, el árbol se centra en vez de quedar pegado
    /// arriba dejando media pantalla muerta debajo. Es lo que se veía al
    /// agrandar la ventana: el árbol arriba y un vacío enorme.
    #[test]
    fn con_espacio_de_sobra_el_arbol_se_centra() {
        let contenido = alto_de_contenido(1, 1.0);
        let disponible = contenido + 400.0;

        let (alto, desplazamiento) = alto_y_centrado(contenido, disponible);

        assert_eq!(alto, disponible, "el lienzo llena el alto disponible");
        assert!(
            (desplazamiento - 200.0).abs() < 0.5,
            "el sobrante se reparte arriba y abajo, no todo abajo: {desplazamiento}"
        );
    }

    /// Y con la ventana baja no se encoge ni se recorta: manda el contenido y
    /// aparece la barra de desplazamiento.
    #[test]
    fn sin_espacio_manda_el_contenido_y_no_se_recorta() {
        let contenido = alto_de_contenido(4, 1.0);

        let (alto, desplazamiento) = alto_y_centrado(contenido, 50.0);

        assert_eq!(alto, contenido);
        assert_eq!(desplazamiento, 0.0);
    }

    /// Con la rama local por delante del remoto, `main` y `origin/main` caen
    /// en commits distintos. Colapsar los dos a "main" etiquetaba ambos igual
    /// y, como los agentes se buscan por nombre de rama, los mismos castores
    /// se dibujaban en los dos: un agente parecía dos.
    #[test]
    fn el_remoto_rezagado_no_se_hace_pasar_por_la_rama_local() {
        // Tal y como los entrega `parse_refs`, que ya desdobla "HEAD -> ".
        let local = nombres_de_rama(&["main".into(), "feat/f30".into()]);
        let remoto = nombres_de_rama(&["origin/main".into(), "origin/HEAD".into()]);

        assert_eq!(local, vec!["main".to_string(), "feat/f30".to_string()]);
        assert_eq!(
            remoto,
            vec!["origin/main".to_string()],
            "un commit que solo tiene el remoto no es la punta de la rama local"
        );
    }

    /// Cuando local y remoto están al día comparten commit, y ahí sí sobra
    /// decirlo dos veces: una etiqueta basta.
    #[test]
    fn local_y_remoto_al_dia_se_dicen_una_sola_vez() {
        let r = nombres_de_rama(&["main".into(), "origin/main".into()]);

        assert_eq!(r, vec!["main".to_string()]);
    }

    /// Un agente solo: justo debajo de su commit, sin desplazarse.
    #[test]
    fn un_agente_sin_subagentes_va_centrado_bajo_su_commit() {
        let r = repartir(&[0], 40.0, 24.0);

        assert_eq!(r.raices, vec![0.0]);
        assert!(r.subagentes[0].is_empty());
    }

    /// Dos raíces se reparten a los lados del commit, no encima.
    #[test]
    fn dos_raices_se_separan_alrededor_del_centro() {
        let r = repartir(&[0, 0], 40.0, 24.0);

        assert_eq!(r.raices, vec![-20.0, 20.0]);
    }

    /// Los subagentes cuelgan centrados bajo su padre: el del medio cae a
    /// plomo y los otros se abren a los lados.
    #[test]
    fn los_subagentes_se_centran_bajo_su_padre() {
        let r = repartir(&[3], 40.0, 24.0);

        assert_eq!(r.raices, vec![0.0]);
        assert_eq!(r.subagentes[0], vec![-24.0, 0.0, 24.0]);
    }

    /// Una raíz con muchos hijos necesita más sitio que una sin ninguno, o
    /// los subagentes de una se meten debajo de la otra.
    #[test]
    fn una_raiz_con_hijos_reserva_el_ancho_que_ocupan() {
        let r = repartir(&[4, 0], 40.0, 24.0);

        // La primera ocupa 4*24 = 96; la segunda, el paso mínimo de 40.
        // Total 136, centrado: la primera va en [-68, 28] y la segunda en
        // [28, 68], así que sus centros son -20 y 48.
        assert_eq!(r.raices, vec![-20.0, 48.0]);

        // Y sus hijos caben dentro de su hueco, sin invadir al vecino.
        let hijos = &r.subagentes[0];
        assert_eq!(hijos.len(), 4);
        assert!(
            hijos.iter().all(|x| *x < 28.0),
            "ningún hijo debe pasarse al hueco de la otra raíz: {hijos:?}"
        );
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
