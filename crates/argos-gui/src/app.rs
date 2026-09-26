use crate::jump::jump_to;
use crate::projects::{nombre_de_proyecto, summarize_projects};
use crate::selector::{ProyectoDisponible, marcar_seleccion};
use crate::theme::{confidence_hint, edad_legible, state_badge, state_label};
use argos_core::discovery::find_repos;
use argos_core::model::AgentState;
use argos_core::monitor::{MonitorConfig, Snapshot};
use argos_core::scope::Scope;
use argos_core::store::{SessionRow, Store};
use argos_core::watcher::{EstadoSondeo, Watcher};
use chrono::Utc;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

const REFRESH: Duration = Duration::from_secs(3);

#[derive(Default, PartialEq, Clone, Copy)]
pub enum Filter {
    All,
    NeedsAttention,
    /// Por defecto: con cientos de sesiones históricas en disco, abrir en
    /// "Todas" entierra lo que está pasando ahora bajo una pared de
    /// subagentes terminados hace días.
    #[default]
    Active,
}

impl Filter {
    pub fn acepta(self, state: AgentState) -> bool {
        match self {
            Filter::All => true,
            Filter::NeedsAttention => state == AgentState::Waiting,
            Filter::Active => matches!(state, AgentState::Waiting | AgentState::Working),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum Pantalla {
    Selector,
    Monitoreo,
}

pub struct ArgosApp {
    watcher: Watcher,
    store: Option<Store>,
    snapshot: Option<Snapshot>,
    pantalla: Pantalla,
    disponibles: Vec<ProyectoDisponible>,
    pub selected: Option<String>,
    pub filter: Filter,
    /// `None` = lista de proyectos vigilados. `Some` = dentro de ese proyecto.
    pub abierto: Option<Option<PathBuf>>,
}

impl ArgosApp {
    pub fn new() -> Self {
        let config = MonitorConfig::default();
        let store = Store::open(&config.db_path).ok();

        // Una selección guardada que ya no existe en disco se ignora sola:
        // `marcar_seleccion` solo lista lo que encontró.
        let guardados = store
            .as_ref()
            .and_then(|s| s.watched().ok())
            .unwrap_or_default();

        let encontrados: Vec<PathBuf> = config
            .search_roots
            .iter()
            .flat_map(|r| find_repos(r, config.max_depth))
            .collect();
        let disponibles = marcar_seleccion(encontrados, &guardados);

        let seleccion: Vec<PathBuf> = disponibles
            .iter()
            .filter(|p| p.seleccionado)
            .map(|p| p.path.clone())
            .collect();

        let (pantalla, abierto) = match seleccion.first() {
            None => (Pantalla::Selector, None),
            Some(p) => (Pantalla::Monitoreo, Some(Some(p.clone()))),
        };

        let config = MonitorConfig {
            scope: Scope::projects(seleccion),
            ..config
        };

        ArgosApp {
            watcher: Watcher::start(config, REFRESH),
            store,
            snapshot: None,
            pantalla,
            disponibles,
            selected: None,
            filter: Filter::default(),
            abierto,
        }
    }

    /// Un clic basta: abre el proyecto y lo recuerda para la próxima vez.
    fn abrir_proyecto(&mut self, path: PathBuf) {
        if let Some(store) = &self.store {
            let _ = store.save_watched(std::slice::from_ref(&path));
        }

        self.watcher.set_scope(Scope::projects(vec![path.clone()]));
        // Directo a sus ramas: pasar por una lista de un solo proyecto sobra.
        self.abierto = Some(Some(path));
        self.selected = None;
        self.pantalla = Pantalla::Monitoreo;
    }

    fn pintar_selector(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("¿Qué proyecto quieres monitorear?");
            ui.weak("Elige uno y Argos leerá solo sus logs.");
            ui.separator();

            let mut abrir: Option<PathBuf> = None;

            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in &self.disponibles {
                    if ui.selectable_label(false, &p.nombre).clicked() {
                        abrir = Some(p.path.clone());
                    }
                }
            });

            // Fuera del recorrido: dentro habría un préstamo vivo de la lista.
            if let Some(path) = abrir {
                self.abrir_proyecto(path);
            }
        });
    }

    /// Solo las sesiones del proyecto abierto.
    fn filas_del_proyecto(
        &self,
        project: &Option<PathBuf>,
        snapshot: &Snapshot,
    ) -> Vec<SessionRow> {
        snapshot
            .rows
            .iter()
            .filter(|r| &r.project == project)
            .cloned()
            .collect()
    }
}

pub struct Grupo {
    pub rama: String,
    pub filas: Vec<SessionRow>,
}

impl Grupo {
    /// El estado más urgente del grupo decide dónde se muestra y si abre solo.
    fn urgencia(&self) -> u8 {
        self.filas
            .iter()
            .map(|f| f.state.urgency())
            .min()
            .unwrap_or(u8::MAX)
    }

    pub fn reclama_atencion(&self) -> bool {
        self.filas.iter().any(|f| f.state == AgentState::Waiting)
    }
}

/// Agrupa por rama y **ordena los grupos por urgencia**, no alfabéticamente:
/// una rama con un agente esperando respuesta tiene que salir arriba, o el
/// tablero deja de responder de un vistazo la pregunta que lo justifica.
///
/// Devuelve filas propias, no referencias: el árbol muta la selección
/// mientras itera, y un préstamo vivo durante el recorrido lo impediría.
pub fn group_by_branch(rows: &[SessionRow], filter: Filter) -> Vec<Grupo> {
    let mut por_rama: BTreeMap<String, Vec<SessionRow>> = BTreeMap::new();

    for row in rows {
        if !filter.acepta(row.state) {
            continue;
        }

        let clave = row
            .branch
            .clone()
            .unwrap_or_else(|| format!("(sin rama) {}", row.anchor_path.display()));
        por_rama.entry(clave).or_default().push(row.clone());
    }

    let mut grupos: Vec<Grupo> = por_rama
        .into_iter()
        .map(|(rama, filas)| Grupo { rama, filas })
        .collect();

    // El nombre desempata para que el orden sea estable entre refrescos.
    grupos.sort_by(|a, b| {
        a.urgencia()
            .cmp(&b.urgencia())
            .then_with(|| a.rama.cmp(&b.rama))
    });

    grupos
}

impl Default for ArgosApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for ArgosApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // No bloquea: si el hilo aún no publicó nada, seguimos con lo último
        // que teníamos. Aquí es donde la ventana deja de congelarse.
        if let Some(s) = self.watcher.latest() {
            self.snapshot = Some(s);
        }
        ctx.request_repaint_after(REFRESH);

        let sesiones = self.snapshot.as_ref().map(|s| s.rows.len()).unwrap_or(0);

        egui::TopBottomPanel::top("encabezado").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Argos");

                if self.pantalla == Pantalla::Monitoreo {
                    if ui.button("Proyectos").clicked() {
                        self.pantalla = Pantalla::Selector;
                    }
                    ui.label(format!("{sesiones} sesiones"));
                }

                if self.watcher.estado() == EstadoSondeo::Detenido {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 90, 90),
                        "el sondeo se detuvo: los datos no se actualizan",
                    );
                }

                if let Some(err) = self
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.persist_error.as_ref())
                {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 90, 90),
                        format!("sin guardar histórico: {err}"),
                    );
                }

                if self.pantalla == Pantalla::Monitoreo {
                    ui.separator();
                    ui.selectable_value(&mut self.filter, Filter::All, "Todas");
                    ui.selectable_value(&mut self.filter, Filter::NeedsAttention, "Me esperan");
                    ui.selectable_value(&mut self.filter, Filter::Active, "Activas");
                }
            });
        });

        if self.pantalla == Pantalla::Selector {
            self.pintar_selector(ctx);
            return;
        }

        let Some(snapshot) = self.snapshot.clone() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.weak("Sondeando…");
            });
            return;
        };

        self.pintar_detalle(ctx, &snapshot);

        match self.abierto.clone() {
            None => self.pintar_proyectos(ctx, &snapshot),
            Some(project) => self.pintar_ramas(ctx, project, &snapshot),
        }
    }
}

impl ArgosApp {
    fn pintar_detalle(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(fila) = snapshot.rows.iter().find(|r| r.id == id).cloned() else {
            return;
        };

        egui::SidePanel::right("detalle")
            .min_width(280.0)
            .show(ctx, |ui| {
                ui.heading(fila.client.label());
                ui.label(state_label(fila.state));
                ui.separator();

                ui.label(format!("Rama: {}", fila.branch.as_deref().unwrap_or("—")));
                ui.label(format!("Ruta: {}", fila.anchor_path.display()));
                if let Some(pid) = fila.pid {
                    ui.label(format!("PID: {pid}"));
                }
                ui.label(format!("Confianza: {:?}", fila.confidence));

                if let Some(m) = fila.metrics {
                    ui.separator();
                    ui.label("Tokens");
                    ui.label(format!("entrada: {}", m.input));
                    ui.label(format!("salida: {}", m.output));
                    ui.label(format!("caché leída: {}", m.cache_read));
                    ui.label(format!("razonamiento: {}", m.thinking));
                    ui.label(format!("total: {}", m.total()));
                } else {
                    ui.separator();
                    ui.weak("Esta plataforma no expone conteo de tokens.");
                }

                ui.separator();
                match fila.warp_focus_url.as_deref() {
                    Some(url) => {
                        if ui.button("Saltar a la sesión en Warp").clicked() {
                            let _ = jump_to(url);
                        }
                    }
                    None => {
                        ui.add_enabled(false, egui::Button::new("Saltar a la sesión en Warp"));
                        ui.weak("Sin pane de Warp asociada.");
                    }
                }
            });
    }

    fn pintar_proyectos(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        let resumen = summarize_projects(&snapshot.rows, self.filter);

        egui::CentralPanel::default().show(ctx, |ui| {
            if resumen.is_empty() {
                ui.weak("Ningún proyecto con sesiones que coincidan con el filtro.");
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in resumen {
                    let (simbolo, color) = state_badge(p.estado);

                    ui.horizontal(|ui| {
                        ui.colored_label(color, simbolo);

                        if ui.selectable_label(false, &p.nombre).clicked() {
                            self.abierto = Some(p.project.clone());
                            self.selected = None;
                        }

                        let mut detalle = Vec::new();
                        if p.esperando > 0 {
                            detalle.push(format!("{} te espera(n)", p.esperando));
                        }
                        if p.trabajando > 0 {
                            detalle.push(format!("{} trabajando", p.trabajando));
                        }
                        if detalle.is_empty() {
                            detalle.push(format!("{} sesion(es)", p.total));
                        }
                        ui.weak(detalle.join(" · "));
                    });
                }
            });
        });
    }

    fn pintar_ramas(&mut self, ctx: &egui::Context, project: Option<PathBuf>, snapshot: &Snapshot) {
        let filas = self.filas_del_proyecto(&project, snapshot);
        let grupos = group_by_branch(&filas, self.filter);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("← Proyectos").clicked() {
                    self.abierto = None;
                    self.selected = None;
                }
                ui.heading(nombre_de_proyecto(project.as_ref()));
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for grupo in grupos {
                    // Lo que te espera se abre solo; lo terminado queda plegado.
                    let abierto = grupo.reclama_atencion();
                    egui::CollapsingHeader::new(&grupo.rama)
                        .default_open(abierto)
                        .show(ui, |ui| {
                            for fila in grupo.filas {
                                let (simbolo, color) = state_badge(fila.state);
                                let sangria = if fila.parent_id.is_some() { 20.0 } else { 0.0 };

                                ui.horizontal(|ui| {
                                    ui.add_space(sangria);
                                    ui.colored_label(color, simbolo);

                                    let etiqueta = format!(
                                        "{} · {}{}",
                                        fila.client.label(),
                                        state_label(fila.state),
                                        confidence_hint(fila.confidence).unwrap_or(""),
                                    );

                                    if ui
                                        .selectable_label(
                                            self.selected.as_deref() == Some(fila.id.as_str()),
                                            etiqueta,
                                        )
                                        .clicked()
                                    {
                                        self.selected = Some(fila.id.clone());
                                    }

                                    let edad = (Utc::now() - fila.last_activity).num_seconds();
                                    ui.weak(edad_legible(edad));
                                });
                            }
                        });
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argos_core::model::{ClientKind, Confidence};
    use chrono::Utc;
    use std::path::PathBuf;

    fn fila(rama: &str, state: AgentState) -> SessionRow {
        SessionRow {
            id: format!("{rama}-{state:?}"),
            client: ClientKind::ClaudeCode,
            anchor_path: PathBuf::from("/repo"),
            project: Some(PathBuf::from("/repo")),
            branch: Some(rama.to_string()),
            warp_focus_url: None,
            pid: None,
            started_at: None,
            last_activity: Utc::now(),
            state,
            confidence: Confidence::High,
            parent_id: None,
            depth: 0,
            metrics: None,
        }
    }

    /// El spec §8: lo que reclama tu atención va arriba. Con orden
    /// alfabético, una rama con un agente esperándote queda enterrada.
    #[test]
    fn los_grupos_se_ordenan_por_urgencia_no_por_nombre() {
        let rows = vec![
            fila("aaa-terminada", AgentState::Finished),
            fila("zzz-te-espera", AgentState::Waiting),
            fila("mmm-trabajando", AgentState::Working),
        ];

        let grupos = group_by_branch(&rows, Filter::All);
        let orden: Vec<&str> = grupos.iter().map(|g| g.rama.as_str()).collect();

        assert_eq!(
            orden,
            vec!["zzz-te-espera", "mmm-trabajando", "aaa-terminada"]
        );
    }

    #[test]
    fn un_grupo_con_alguien_esperando_reclama_atencion() {
        let grupos = group_by_branch(&[fila("x", AgentState::Waiting)], Filter::All);
        assert!(grupos[0].reclama_atencion());

        let grupos = group_by_branch(&[fila("y", AgentState::Finished)], Filter::All);
        assert!(!grupos[0].reclama_atencion());
    }

    #[test]
    fn el_filtro_me_esperan_deja_solo_lo_bloqueado() {
        let rows = vec![
            fila("a", AgentState::Waiting),
            fila("b", AgentState::Working),
            fila("c", AgentState::Finished),
        ];

        let grupos = group_by_branch(&rows, Filter::NeedsAttention);
        assert_eq!(grupos.len(), 1);
        assert_eq!(grupos[0].rama, "a");
    }

    #[test]
    fn ramas_con_la_misma_urgencia_conservan_orden_estable_por_nombre() {
        let rows = vec![
            fila("zzz", AgentState::Working),
            fila("aaa", AgentState::Working),
        ];

        let grupos = group_by_branch(&rows, Filter::All);
        let orden: Vec<&str> = grupos.iter().map(|g| g.rama.as_str()).collect();
        assert_eq!(orden, vec!["aaa", "zzz"]);
    }
}
