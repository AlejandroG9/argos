use crate::jump::jump_to;
use crate::nodos::{construir_grafo, pintar_grafo};
use crate::projects::{nombre_de_proyecto, summarize_projects};
use crate::selector::{ProyectoDisponible, marcar_seleccion};
use crate::theme::{espacio, state_badge, state_label};
use crate::ventana::Ventana;
use argos_core::discovery::find_repos;
use argos_core::model::AgentState;
use argos_core::monitor::{MonitorConfig, Snapshot};
use argos_core::scope::Scope;
use argos_core::store::{SessionRow, Store};
use argos_core::watcher::{EstadoSondeo, Watcher};
use chrono::Utc;
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
    pub ventana: Ventana,
    zoom: f32,
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
            ventana: Ventana::default(),
            zoom: 1.0,
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

impl Default for ArgosApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for ArgosApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::theme::aplicar_estilo(ctx);

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
                    ui.label(crate::theme::plural(sesiones, "sesión", "sesiones"));
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

            if self.pantalla == Pantalla::Monitoreo {
                ui.horizontal(|ui| {
                    for v in [Ventana::Hoy, Ventana::Dias7, Ventana::Dias30, Ventana::Todo] {
                        ui.selectable_value(&mut self.ventana, v, v.etiqueta());
                    }
                });
            }
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
        let resumen = summarize_projects(&snapshot.rows, self.filter, self.ventana, Utc::now());

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
                            detalle.push(if p.esperando == 1 {
                                "1 te espera".to_string()
                            } else {
                                format!("{} te esperan", p.esperando)
                            });
                        }
                        if p.trabajando > 0 {
                            detalle.push(format!("{} trabajando", p.trabajando));
                        }
                        if detalle.is_empty() {
                            detalle.push(crate::theme::plural(p.total, "sesión", "sesiones"));
                        }
                        ui.weak(detalle.join(" · "));
                    });
                }
            });
        });
    }

    fn pintar_ramas(&mut self, ctx: &egui::Context, project: Option<PathBuf>, snapshot: &Snapshot) {
        let filas = self.filas_del_proyecto(&project, snapshot);
        let grafo = construir_grafo(&filas, self.filter, self.ventana, Utc::now());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("← Proyectos").clicked() {
                    self.pantalla = Pantalla::Selector;
                }
                ui.heading(nombre_de_proyecto(project.as_ref()));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("+").clicked() {
                        self.zoom = (self.zoom * 1.15).min(2.0);
                    }
                    if ui.small_button("−").clicked() {
                        self.zoom = (self.zoom / 1.15).max(0.5);
                    }
                    ui.weak(format!("{:.0}%", self.zoom * 100.0));
                });
            });
            ui.add_space(espacio::S);

            if grafo.nodos.is_empty() {
                ui.weak("Nada que mostrar con los filtros actuales.");
                return;
            }

            egui::ScrollArea::both()
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .show(ui, |ui| {
                    if let Some(id) = pintar_grafo(ui, &grafo, self.selected.as_deref(), self.zoom)
                    {
                        self.selected = Some(id);
                    }
                });
        });
    }
}
