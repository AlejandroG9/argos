use crate::git_vista::{ancho_estimado, pintar_git, tender_carriles};
use crate::jump::jump_to;
use crate::nodos::{construir_grafo, pintar_grafo};
use crate::projects::{nombre_de_proyecto, summarize_projects};
use crate::selector::{ProyectoDisponible, filtrar_proyectos, marcar_seleccion};
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

#[derive(Default, PartialEq, Clone, Copy)]
pub enum Vista {
    #[default]
    Git,
    Agentes,
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
    pub vista: Vista,
    /// Commit seleccionado en la vista de git.
    pub commit_abierto: Option<String>,
    /// La vista de git abre por el extremo reciente; solo la primera vez,
    /// para no arrastrar al usuario de vuelta cada refresco.
    git_centrado: bool,
    logos: crate::logos::Logos,
    mascota: crate::mascota::Mascota,
    /// Peticiones ya recuperadas, por (commit, sesión). Leer un log de
    /// sesión cuesta, así que se hace al pulsar y se recuerda.
    prompts: std::collections::HashMap<(String, String), Option<String>>,
    zoom: f32,
    /// `None` = lista de proyectos vigilados. `Some` = dentro de ese proyecto.
    pub abierto: Option<Option<PathBuf>>,
    selector_query: String,
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
            vista: Vista::default(),
            commit_abierto: None,
            git_centrado: false,
            logos: crate::logos::Logos::default(),
            mascota: crate::mascota::Mascota::default(),
            prompts: std::collections::HashMap::new(),
            zoom: 1.0,
            abierto,
            selector_query: String::new(),
        }
    }

    /// Un clic basta: abre el proyecto y lo recuerda para la próxima vez.
    fn abrir_proyecto(&mut self, path: PathBuf) {
        if let Some(store) = &self.store {
            let _ = store.save_watched(std::slice::from_ref(&path));
        }

        self.watcher.set_scope(Scope::projects(vec![path.clone()]));
        for proyecto in &mut self.disponibles {
            proyecto.seleccionado = proyecto.path == path;
        }
        // Directo a sus ramas: pasar por una lista de un solo proyecto sobra.
        self.abierto = Some(Some(path));
        self.selected = None;
        self.git_centrado = false;
        self.pantalla = Pantalla::Monitoreo;
    }

    fn pintar_selector(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(espacio::L);
            ui.label(
                egui::RichText::new("¿Qué proyecto quieres monitorear?")
                    .font(crate::tipografia::display(28.0))
                    .color(crate::theme::color::TEXTO),
            );
            ui.add_space(espacio::XS);
            ui.weak("Elige uno y Argos leerá solo sus registros.");
            ui.add_space(espacio::M);

            let buscador_id = ui.make_persistent_id("buscar-proyecto");
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                ui.memory_mut(|m| m.request_focus(buscador_id));
            }

            let mut confirmar_unico = false;
            ui.horizontal(|ui| {
                let respuesta = egui::Frame::NONE
                    .fill(crate::theme::color::SUPERFICIE)
                    .stroke(egui::Stroke::new(1.0, crate::theme::color::BORDE))
                    .corner_radius(crate::theme::REDONDEO)
                    .inner_margin(egui::Margin::symmetric(espacio::M as i8, espacio::S as i8))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.selector_query)
                                .id(buscador_id)
                                .hint_text("Buscar por nombre o ruta…")
                                .desired_width(336.0)
                                .frame(false),
                        )
                    })
                    .inner;
                confirmar_unico =
                    respuesta.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                respuesta.on_hover_text("Buscar proyectos · ⌘F");

                if !self.selector_query.is_empty() && ui.small_button("Limpiar").clicked() {
                    self.selector_query.clear();
                    ui.memory_mut(|m| m.request_focus(buscador_id));
                }
            });
            ui.add_space(espacio::S);

            let mut abrir: Option<PathBuf> = None;
            let visibles = filtrar_proyectos(&self.disponibles, &self.selector_query);

            ui.horizontal(|ui| {
                ui.small(if self.selector_query.trim().is_empty() {
                    crate::theme::plural(visibles.len(), "proyecto", "proyectos")
                } else {
                    format!("{} de {}", visibles.len(), self.disponibles.len())
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("selecciona para abrir");
                });
            });
            ui.add_space(espacio::XS);

            if visibles.is_empty() {
                crate::theme::estado_vacio(
                    ui,
                    "No hay proyectos que coincidan",
                    "Prueba con otro nombre o una parte de la ruta.",
                );
                return;
            }

            if confirmar_unico && visibles.len() == 1 {
                abrir = Some(visibles[0].path.clone());
            }

            egui::ScrollArea::vertical()
                .id_salt("selector-proyectos")
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = espacio::XS;

                    for p in visibles {
                        let fila = crate::theme::fila_proyecto(
                            ui,
                            &p.nombre,
                            &crate::theme::ruta_corta(&p.path),
                            p.seleccionado,
                        );
                        if fila.clicked() {
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
        // Repintado continuo solo si hay un agente activo que animar: esta
        // app vive abierta en segundo plano y no puede gastar CPU dibujando
        // un árbol que no se mueve.
        let hay_actividad = self.snapshot.as_ref().is_some_and(|s| {
            s.rows
                .iter()
                .any(|r| matches!(r.state, AgentState::Working | AgentState::Waiting))
        });

        if hay_actividad && self.pantalla == Pantalla::Monitoreo {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(REFRESH);
        }

        let sesiones = self.snapshot.as_ref().map(|s| s.rows.len()).unwrap_or(0);

        egui::TopBottomPanel::top("encabezado")
            .frame(
                egui::Frame::NONE
                    .fill(crate::theme::color::SUPERFICIE)
                    .inner_margin(espacio::M),
            )
            .show(ctx, |ui| {
                // Primera zona: dónde estoy y qué miro.
                ui.horizontal(|ui| {
                    if self.pantalla == Pantalla::Monitoreo {
                        if ui.button("←").on_hover_text("Volver a proyectos").clicked() {
                            self.pantalla = Pantalla::Selector;
                        }
                        ui.add_space(espacio::XS);
                        ui.heading(nombre_de_proyecto(
                            self.abierto.as_ref().and_then(|p| p.as_ref()),
                        ));
                        ui.add_space(espacio::M);
                        // El hueco de la barra se gana con la respuesta a la
                        // pregunta por la que se abre la app, no se rellena:
                        // cuántos te esperan y cuántos trabajan.
                        self.pintar_resumen(ui, sesiones);
                    } else {
                        // La única aparición del cobre en la interfaz: la
                        // marca junto a su nombre, en el display del diseño.
                        let alto = ui.text_style_height(&egui::TextStyle::Heading);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(alto, alto), egui::Sense::hover());
                        crate::theme::pintar_marca(ui.painter(), rect.center(), alto * 0.5);
                        ui.add_space(espacio::XS);
                        ui.label(
                            egui::RichText::new("Argos")
                                .font(crate::tipografia::display(26.0))
                                .color(crate::theme::color::TEXTO),
                        );
                        ui.add_space(espacio::L);
                        // La barra de esta pantalla no tiene controles, así que
                        // en vez de dejarla vacía dice qué promete la app.
                        ui.weak("qué agente, de qué compañía, en qué rama");
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.pantalla == Pantalla::Monitoreo {
                            if ui.small_button("+").clicked() {
                                self.zoom = (self.zoom * 1.15).min(2.0);
                            }
                            if ui.small_button("−").clicked() {
                                self.zoom = (self.zoom / 1.15).max(0.5);
                            }
                            ui.weak(format!("{:.0}%", self.zoom * 100.0));
                            ui.add_space(espacio::L);

                            crate::theme::chip_valor(
                                ui,
                                &mut self.vista,
                                Vista::Agentes,
                                "Agentes",
                            );
                            crate::theme::chip_valor(ui, &mut self.vista, Vista::Git, "Git");
                        } else {
                            ui.weak(crate::theme::plural(
                                self.disponibles.len(),
                                "proyecto",
                                "proyectos",
                            ));
                        }

                        for aviso in self.avisos() {
                            ui.colored_label(egui::Color32::from_rgb(210, 90, 90), aviso);
                        }
                    });
                });

                // Segunda zona: refinar lo que ya se está mirando. Más tenue
                // a propósito — es ajuste, no navegación.
                if self.pantalla == Pantalla::Monitoreo {
                    ui.add_space(espacio::S);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = espacio::XS;
                        ui.small("estado");
                        ui.add_space(espacio::XS);
                        crate::theme::chip_valor(ui, &mut self.filter, Filter::Active, "activas");
                        crate::theme::chip_valor(
                            ui,
                            &mut self.filter,
                            Filter::NeedsAttention,
                            "me esperan",
                        );
                        crate::theme::chip_valor(ui, &mut self.filter, Filter::All, "todas");

                        // "cuándo" se ancla a la derecha: los dos grupos
                        // sujetan la fila por sus extremos en vez de
                        // amontonarse a la izquierda con medio ancho vacío.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = espacio::XS;
                            // Invertido: este diseño coloca de derecha a
                            // izquierda, y el orden que se lee es el de arriba.
                            for v in [Ventana::Todo, Ventana::Dias30, Ventana::Dias7, Ventana::Hoy]
                            {
                                crate::theme::chip_valor(ui, &mut self.ventana, v, v.etiqueta());
                            }
                            ui.add_space(espacio::XS);
                            ui.small("cuándo");
                        });
                    });
                }
            });

        if self.pantalla == Pantalla::Selector {
            self.pintar_selector(ctx);
            return;
        }

        let Some(snapshot) = self.snapshot.clone() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                crate::theme::estado_vacio(
                    ui,
                    "Leyendo el proyecto…",
                    "El primer sondeo recorre los registros de cada plataforma.",
                );
            });
            return;
        };

        match self.vista {
            Vista::Git => self.pintar_detalle_commit(ctx, &snapshot),
            Vista::Agentes => self.pintar_detalle(ctx, &snapshot),
        }

        match self.abierto.clone() {
            None => self.pintar_proyectos(ctx, &snapshot),
            Some(project) => self.pintar_ramas(ctx, project, &snapshot),
        }
    }
}

impl ArgosApp {
    /// El resumen de la barra: símbolo de estado y cuenta, en su color.
    ///
    /// Sin nada activo cae a la cuenta de sesiones: decir "no pasa nada" con
    /// un espacio en blanco se confunde con que la app no ha cargado.
    fn pintar_resumen(&self, ui: &mut egui::Ui, sesiones: usize) {
        let filas = self
            .snapshot
            .as_ref()
            .map(|s| s.rows.as_slice())
            .unwrap_or_default();

        let resumen = crate::projects::resumen_de_estado(filas);
        if resumen.is_empty() {
            ui.weak(crate::theme::plural(sesiones, "sesión", "sesiones"));
            return;
        }

        ui.spacing_mut().item_spacing.x = espacio::XS;
        for (i, (estado, texto)) in resumen.iter().enumerate() {
            if i > 0 {
                ui.add_space(espacio::S);
            }
            let (simbolo, color) = state_badge(*estado);
            ui.colored_label(color, simbolo);
            ui.label(egui::RichText::new(texto).size(12.5).color(color));
        }
    }

    /// Lo que va mal ahora mismo, en rojo y en la barra: un sondeo muerto o
    /// un histórico que no se guarda serían invisibles de otro modo.
    fn avisos(&self) -> Vec<String> {
        let mut v = Vec::new();

        if self.watcher.estado() == EstadoSondeo::Detenido {
            v.push("el sondeo se detuvo".to_string());
        }
        if let Some(e) = self
            .snapshot
            .as_ref()
            .and_then(|s| s.persist_error.as_ref())
        {
            v.push(format!("sin guardar histórico: {e}"));
        }
        if let Some(n) = self
            .snapshot
            .as_ref()
            .map(|s| s.degraded.len())
            .filter(|n| *n > 0)
        {
            v.push(crate::theme::plural(
                n,
                "plataforma degradada",
                "plataformas degradadas",
            ));
        }

        v
    }

    fn pintar_detalle_commit(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        let Some(sha) = self.commit_abierto.clone() else {
            return;
        };
        let Some(commit) = snapshot.commits.iter().find(|c| c.sha == sha).cloned() else {
            return;
        };

        egui::SidePanel::right("detalle-commit")
            .min_width(320.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.monospace(&commit.sha);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            self.commit_abierto = None;
                        }
                    });
                });
                ui.add_space(espacio::S);
                ui.label(&commit.mensaje);
                ui.add_space(espacio::M);

                ui.weak(format!(
                    "{} · {}",
                    commit.autor,
                    commit.fecha.format("%Y-%m-%d %H:%M")
                ));
                if commit.es_merge() {
                    ui.weak(format!("merge de {} padres", commit.padres.len()));
                }

                let ramas = crate::git_vista::nombres_de_rama(&commit.refs);
                if !ramas.is_empty() {
                    ui.add_space(espacio::S);
                    ui.weak(format!("en {}", ramas.join(", ")));
                }

                ui.separator();
                ui.strong("Conversaciones que lo mencionan");

                let sesiones = snapshot.menciones.sesiones_de(&commit.sha);
                if sesiones.is_empty() {
                    ui.weak("Ninguna sesión de agente menciona este commit.");
                    return;
                }

                // "Menciona" y no "creó": una sesión que corrió `git log`
                // menciona commits que no hizo. Ver `atribucion` en el núcleo.
                for id in sesiones {
                    let fila = snapshot.rows.iter().find(|r| r.id == id).cloned();
                    ui.add_space(espacio::M);

                    let Some(f) = fila else {
                        ui.weak(format!("sesión {id} (fuera de la vista actual)"));
                        continue;
                    };

                    ui.horizontal(|ui| {
                        let (simbolo, color) = state_badge(f.state);
                        ui.colored_label(color, simbolo);
                        ui.label(f.client.label());
                        ui.weak(state_label(f.state));
                    });

                    if let Some(m) = f.metrics {
                        ui.weak(format!("{} tokens en la sesión", m.total()));
                    }

                    // La petición que llevó a este commit: el porqué, que no
                    // está en git. Se lee al pulsar y se recuerda.
                    let clave = (commit.sha.clone(), f.id.clone());
                    let prompt = self
                        .prompts
                        .entry(clave)
                        .or_insert_with(|| {
                            f.source_path.as_ref().and_then(|ruta| {
                                std::fs::read_to_string(ruta).ok().and_then(|t| {
                                    argos_core::atribucion::prompt_previo(&t, &commit.sha)
                                })
                            })
                        })
                        .clone();

                    if let Some(texto) = prompt {
                        ui.add_space(espacio::S);
                        ui.weak("lo pediste así:");
                        let recorte: String = texto.chars().take(400).collect();
                        ui.label(egui::RichText::new(recorte).italics());
                    }
                }
            });
    }

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
        let now = Utc::now();

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.vista {
                Vista::Git => {
                    let commits: Vec<_> = snapshot
                        .commits
                        .iter()
                        .filter(|c| self.ventana.acepta(c.fecha, now))
                        .cloned()
                        .collect();

                    if commits.is_empty() {
                        crate::theme::estado_vacio(
                            ui,
                            "Sin commits en esta ventana",
                            "Prueba a ampliar el rango de tiempo arriba.",
                        );
                        return;
                    }

                    let grafo = tender_carriles(&commits);
                    // Se mide aquí, fuera del `ScrollArea`: dentro ya no es el
                    // alto de la ventana y el árbol quedaría pegado arriba.
                    let alto_disponible = ui.available_height();
                    let mut area = egui::ScrollArea::both()
                        .scroll_source(egui::scroll_area::ScrollSource::ALL);

                    // Lo reciente está a la derecha y es lo que se viene a
                    // ver, así que la vista abre ahí en vez de en el commit
                    // inicial del repo. Solo la primera vez: después manda
                    // el usuario.
                    if !self.git_centrado {
                        area = area.horizontal_scroll_offset(ancho_estimado(&grafo, self.zoom));
                        self.git_centrado = true;
                    }

                    area.show(ui, |ui| {
                        let mut pintura = crate::git_vista::Pintura {
                            estado_ramas: &snapshot.ramas,
                            filas: &snapshot.rows,
                            logos: &mut self.logos,
                            mascota: &mut self.mascota,
                            seleccionado: self.commit_abierto.as_deref(),
                            now,
                            zoom: self.zoom,
                            alto_disponible,
                        };

                        match pintar_git(ui, &grafo, &mut pintura) {
                            Some(crate::git_vista::Pulsado::Commit(sha)) => {
                                self.commit_abierto = Some(sha);
                            }
                            Some(crate::git_vista::Pulsado::Agente(id)) => {
                                // A la terminal de ese agente. Sin pane
                                // asociada no hay a dónde ir, y el globo del
                                // castor ya lo advierte antes del clic.
                                if let Some(url) = snapshot
                                    .rows
                                    .iter()
                                    .find(|r| r.id == id)
                                    .and_then(|r| r.warp_focus_url.as_deref())
                                {
                                    let _ = jump_to(url);
                                }
                            }
                            None => {}
                        }
                    });
                }
                Vista::Agentes => {
                    let filas = self.filas_del_proyecto(&project, snapshot);
                    let grafo = construir_grafo(&filas, self.filter, self.ventana, now);

                    if grafo.nodos.is_empty() {
                        crate::theme::estado_vacio(
                            ui,
                            "Ningún agente coincide",
                            "Cambia el filtro de estado o amplía el rango de tiempo.",
                        );
                        return;
                    }

                    egui::ScrollArea::both()
                        .scroll_source(egui::scroll_area::ScrollSource::ALL)
                        .show(ui, |ui| {
                            if let Some(id) =
                                pintar_grafo(ui, &grafo, self.selected.as_deref(), self.zoom)
                            {
                                self.selected = Some(id);
                            }
                        });
                }
            }
        });
    }
}
