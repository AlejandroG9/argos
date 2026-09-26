use crate::jump::jump_to;
use crate::theme::{confidence_hint, state_badge, state_label};
use argos_core::model::AgentState;
use argos_core::monitor::{Monitor, MonitorConfig, Snapshot};
use argos_core::store::SessionRow;
use chrono::Utc;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(3);

#[derive(Default, PartialEq, Clone, Copy)]
pub enum Filter {
    #[default]
    All,
    NeedsAttention,
    Active,
}

pub struct ArgosApp {
    monitor: Monitor,
    snapshot: Snapshot,
    last_poll: Instant,
    pub selected: Option<String>,
    pub filter: Filter,
}

impl ArgosApp {
    pub fn new() -> Self {
        let monitor = Monitor::new(MonitorConfig::default());
        let snapshot = monitor.poll();
        ArgosApp {
            monitor,
            snapshot,
            last_poll: Instant::now(),
            selected: None,
            filter: Filter::default(),
        }
    }

    fn refresh_if_due(&mut self) {
        if self.last_poll.elapsed() >= REFRESH {
            self.snapshot = self.monitor.poll();
            self.last_poll = Instant::now();
        }
    }

    fn by_branch(&self) -> Vec<Grupo> {
        group_by_branch(&self.snapshot.rows, self.filter)
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
        let visible = match filter {
            Filter::All => true,
            Filter::NeedsAttention => row.state == AgentState::Waiting,
            Filter::Active => matches!(row.state, AgentState::Waiting | AgentState::Working),
        };
        if !visible {
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
        self.refresh_if_due();
        ctx.request_repaint_after(REFRESH);

        egui::TopBottomPanel::top("encabezado").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Argos");
                ui.label(format!("{} sesiones", self.snapshot.rows.len()));
                if !self.snapshot.degraded.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 120, 60),
                        format!(
                            "{} plataforma(s) degradada(s)",
                            self.snapshot.degraded.len()
                        ),
                    );
                }
                ui.separator();
                ui.selectable_value(&mut self.filter, Filter::All, "Todas");
                ui.selectable_value(&mut self.filter, Filter::NeedsAttention, "Me esperan");
                ui.selectable_value(&mut self.filter, Filter::Active, "Activas");
            });
        });

        if let Some(id) = self.selected.clone()
            && let Some(fila) = self.snapshot.rows.iter().find(|r| r.id == id).cloned()
        {
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

        let grupos = self.by_branch();

        egui::CentralPanel::default().show(ctx, |ui| {
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

                                    let edad =
                                        (Utc::now() - fila.last_activity).num_seconds().max(0);
                                    ui.weak(format!("hace {edad}s"));
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
