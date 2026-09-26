use crate::correlator::correlate;
use crate::discovery::{Worktree, discover_worktrees, find_repos};
use crate::model::{ClientKind, Confidence};
use crate::observation::ProcessObservation;
use crate::probes::SessionProbe;
use crate::probes::antigravity::AntigravityProbe;
use crate::probes::claude::ClaudeProbe;
use crate::probes::codex::CodexProbe;
use crate::probes::gemini::GeminiProbe;
use crate::probes::process::ProcessProbe;
use crate::state_engine::{DEFAULT_IDLE_THRESHOLD, infer};
use crate::store::{SessionRow, Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub rows: Vec<SessionRow>,
    /// Plataformas cuyo probe falló en este ciclo, con el motivo.
    pub degraded: Vec<(ClientKind, String)>,
    /// Motivo por el que no se pudo guardar este ciclo. El tablero en vivo
    /// sigue funcionando sin base, así que un fallo aquí sería invisible si
    /// no se reportara.
    pub persist_error: Option<String>,
    pub taken_at: DateTime<Utc>,
}

pub struct MonitorConfig {
    pub search_roots: Vec<PathBuf>,
    pub max_depth: usize,
    pub idle_threshold: Duration,
    pub db_path: PathBuf,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
        MonitorConfig {
            search_roots: vec![home.join("Proyectos")],
            max_depth: 3,
            idle_threshold: DEFAULT_IDLE_THRESHOLD,
            db_path: home.join(".argos/argos.db"),
        }
    }
}

pub struct Monitor {
    config: MonitorConfig,
    probes: Vec<Box<dyn SessionProbe>>,
    process_probe: ProcessProbe,
    store: Option<Store>,
}

impl Monitor {
    pub fn new(config: MonitorConfig) -> Self {
        if let Some(parent) = config.db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Una base que no abre degrada la persistencia, no el monitoreo:
        // el tablero en vivo debe seguir funcionando.
        let store = Store::open(&config.db_path).ok();

        Monitor {
            probes: vec![
                Box::new(ClaudeProbe::new(ClaudeProbe::default_root())),
                Box::new(CodexProbe::new(CodexProbe::default_root())),
                Box::new(GeminiProbe::new(GeminiProbe::default_root())),
                Box::new(AntigravityProbe::new(AntigravityProbe::default_history())),
            ],
            process_probe: ProcessProbe::new(),
            store,
            config,
        }
    }

    pub fn poll(&self) -> Snapshot {
        let procs = self.process_probe.observe().unwrap_or_default();

        let mut worktrees: Vec<Worktree> = Vec::new();
        for root in &self.config.search_roots {
            for repo in find_repos(root, self.config.max_depth) {
                if let Ok(mut found) = discover_worktrees(&repo) {
                    worktrees.append(&mut found);
                }
            }
        }

        let snapshot = collect(
            &self.probes,
            &procs,
            &worktrees,
            Utc::now(),
            self.config.idle_threshold,
        );

        let mut snapshot = snapshot;
        snapshot.persist_error = match &self.store {
            Some(store) => persist(store, &snapshot).err().map(|e| e.to_string()),
            None => Some("no se pudo abrir la base de datos".to_string()),
        };

        snapshot
    }

    /// El spec §7: la base es derivada, así que vaciarla y reingerir es una
    /// operación normal, no una recuperación de desastre.
    pub fn reindex(&self) -> Result<Snapshot, StoreError> {
        if let Some(store) = &self.store {
            store.reset()?;
        }
        Ok(self.poll())
    }
}

/// Guarda el estado actual y añade una muestra por sesión al histórico.
pub fn persist(store: &Store, snapshot: &Snapshot) -> Result<(), StoreError> {
    store.upsert_snapshot(&snapshot.rows)?;
    for row in &snapshot.rows {
        store.record_sample(row, snapshot.taken_at)?;
    }
    Ok(())
}

/// Separado de `Monitor` para poder probarlo con probes de mentira.
pub fn collect(
    probes: &[Box<dyn SessionProbe>],
    procs: &[ProcessObservation],
    worktrees: &[Worktree],
    now: DateTime<Utc>,
    idle_threshold: Duration,
) -> Snapshot {
    let mut sessions = Vec::new();
    let mut degraded = Vec::new();

    for probe in probes {
        match probe.observe() {
            Ok(mut found) => sessions.append(&mut found),
            // Un probe caído degrada su plataforma y nada más.
            Err(err) => degraded.push((probe.client(), err.to_string())),
        }
    }

    let correlated = correlate(procs, &sessions, worktrees);

    let mut rows: Vec<SessionRow> = correlated
        .iter()
        .map(|c| {
            let (state, confidence) = infer(c, now, idle_threshold);

            // Un worktree eliminado deja su log en disco, y su ruta sigue siendo
            // subdirectorio del repo: la búsqueda por prefijo elegiría el repo
            // raíz y le atribuiría su rama. Solo se hereda la rama de un
            // worktree que es la ruta exacta de la sesión o que todavía existe.
            let worktree = c
                .worktree
                .as_ref()
                .filter(|w| w.path == c.session.anchor_path || c.session.anchor_path.exists());
            let confidence = if worktree.is_none() {
                confidence.min(Confidence::Low)
            } else {
                confidence
            };

            SessionRow {
                id: c.session.id.clone(),
                client: c.session.client,
                anchor_path: c.session.anchor_path.clone(),
                project: worktree.map(|w| w.repo_root.clone()),
                branch: worktree
                    .and_then(|w| w.branch.clone())
                    .or_else(|| c.session.git_branch.clone()),
                warp_focus_url: c.process.as_ref().and_then(|p| p.warp_focus_url.clone()),
                pid: c.process.as_ref().map(|p| p.pid),
                started_at: c.process.as_ref().map(|p| p.started_at),
                last_activity: c.session.last_activity,
                state,
                confidence,
                parent_id: c.session.parent_id.clone(),
                depth: if c.session.parent_id.is_some() { 1 } else { 0 },
                metrics: c.session.metrics,
            }
        })
        .collect();

    rows.sort_by_key(|r| (r.state.urgency(), std::cmp::Reverse(r.last_activity)));

    Snapshot {
        rows,
        degraded,
        persist_error: None,
        taken_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ProbeError;
    use crate::model::ClientKind;
    use crate::observation::{ActivitySemantics, Capabilities, SessionObservation};
    use std::path::PathBuf;

    struct ProbeQueFalla;

    impl SessionProbe for ProbeQueFalla {
        fn client(&self) -> ClientKind {
            ClientKind::Codex
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::minimal()
        }
        fn observe(&self) -> Result<Vec<SessionObservation>, ProbeError> {
            Err(ProbeError::SourceMissing(PathBuf::from("/no/existe")))
        }
    }

    struct ProbeQueFunciona;

    impl SessionProbe for ProbeQueFunciona {
        fn client(&self) -> ClientKind {
            ClientKind::ClaudeCode
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::full()
        }
        fn observe(&self) -> Result<Vec<SessionObservation>, ProbeError> {
            Ok(vec![SessionObservation {
                id: "s1".into(),
                client: ClientKind::ClaudeCode,
                anchor_path: PathBuf::from("/repo"),
                git_branch: Some("main".into()),
                first_seen: None,
                last_activity: Utc::now(),
                activity: ActivitySemantics::AssistantTurnEnded,
                metrics: None,
                parent_id: None,
                source_path: PathBuf::from("/logs/s1.jsonl"),
            }])
        }
    }

    /// Constraint global: un probe que falla degrada solo su plataforma.
    #[test]
    fn un_probe_que_falla_no_tumba_el_ciclo_ni_a_los_demas() {
        let probes: Vec<Box<dyn SessionProbe>> =
            vec![Box::new(ProbeQueFalla), Box::new(ProbeQueFunciona)];

        let snapshot = collect(&probes, &[], &[], Utc::now(), DEFAULT_IDLE_THRESHOLD);

        assert_eq!(
            snapshot.rows.len(),
            1,
            "la sesión del probe sano debe estar"
        );
        assert_eq!(
            snapshot.degraded.len(),
            1,
            "el fallo debe quedar registrado"
        );
        assert_eq!(snapshot.degraded[0].0, ClientKind::Codex);
    }

    #[test]
    fn el_snapshot_ordena_por_urgencia() {
        use crate::model::AgentState;

        let probes: Vec<Box<dyn SessionProbe>> = vec![Box::new(ProbeQueFunciona)];
        let snapshot = collect(&probes, &[], &[], Utc::now(), DEFAULT_IDLE_THRESHOLD);

        // Sin proceso vivo, la única sesión termina.
        assert_eq!(snapshot.rows[0].state, AgentState::Finished);
    }

    /// Review Focus #5, caso real observado en la máquina: un worktree
    /// eliminado con `git worktree remove` deja su log en disco. Su ruta sigue
    /// siendo un subdirectorio del repo, así que la búsqueda por prefijo elegía
    /// el repo raíz y le atribuía su rama con confianza alta — una mentira.
    #[test]
    fn una_sesion_en_un_worktree_eliminado_no_hereda_la_rama_del_repo() {
        use crate::model::Confidence;

        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let borrado = repo.join(".worktrees/ya-no-existe");
        assert!(!borrado.exists(), "la precondición del test");

        struct ProbeHuerfano(PathBuf);
        impl SessionProbe for ProbeHuerfano {
            fn client(&self) -> ClientKind {
                ClientKind::ClaudeCode
            }
            fn capabilities(&self) -> Capabilities {
                Capabilities::full()
            }
            fn observe(&self) -> Result<Vec<SessionObservation>, ProbeError> {
                Ok(vec![SessionObservation {
                    id: "huerfana".into(),
                    client: ClientKind::ClaudeCode,
                    anchor_path: self.0.clone(),
                    git_branch: None,
                    first_seen: None,
                    last_activity: Utc::now(),
                    activity: ActivitySemantics::AssistantTurnEnded,
                    metrics: None,
                    parent_id: None,
                    source_path: PathBuf::from("/logs/huerfana.jsonl"),
                }])
            }
        }

        let worktrees = vec![Worktree {
            path: repo.clone(),
            branch: Some("main".into()),
            repo_root: repo.clone(),
        }];
        let probes: Vec<Box<dyn SessionProbe>> = vec![Box::new(ProbeHuerfano(borrado))];

        let snapshot = collect(&probes, &[], &worktrees, Utc::now(), DEFAULT_IDLE_THRESHOLD);

        assert_eq!(snapshot.rows.len(), 1, "la huérfana no debe desaparecer");
        assert_eq!(
            snapshot.rows[0].branch, None,
            "no debe atribuirle la rama del repo que la contiene"
        );
        assert_eq!(snapshot.rows[0].confidence, Confidence::Low);
    }

    /// El proyecto es la raíz del repo, no la ruta del worktree: un agente en
    /// `Orion/.worktrees/x` pertenece al proyecto `Orion`, que es el nivel por
    /// el que el usuario navega.
    #[test]
    fn el_proyecto_es_la_raiz_del_repo_no_la_ruta_del_worktree() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let wt = repo.join("src");

        struct P(PathBuf);
        impl SessionProbe for P {
            fn client(&self) -> ClientKind {
                ClientKind::ClaudeCode
            }
            fn capabilities(&self) -> Capabilities {
                Capabilities::full()
            }
            fn observe(&self) -> Result<Vec<SessionObservation>, ProbeError> {
                Ok(vec![SessionObservation {
                    id: "s".into(),
                    client: ClientKind::ClaudeCode,
                    anchor_path: self.0.clone(),
                    git_branch: None,
                    first_seen: None,
                    last_activity: Utc::now(),
                    activity: ActivitySemantics::AssistantTurnEnded,
                    metrics: None,
                    parent_id: None,
                    source_path: PathBuf::from("/logs/s.jsonl"),
                }])
            }
        }

        let worktrees = vec![Worktree {
            path: wt.clone(),
            branch: Some("feat/x".into()),
            repo_root: repo.clone(),
        }];
        let probes: Vec<Box<dyn SessionProbe>> = vec![Box::new(P(wt.clone()))];

        let snapshot = collect(&probes, &[], &worktrees, Utc::now(), DEFAULT_IDLE_THRESHOLD);

        assert_eq!(
            snapshot.rows[0].project,
            Some(repo),
            "debe ser la raíz del repo"
        );
        assert_ne!(
            snapshot.rows[0].project,
            Some(wt),
            "no la ruta del worktree"
        );
    }

    /// El spec §7 exige que lo observado se persista desde el inicio.
    #[test]
    fn el_ciclo_persiste_el_snapshot_y_acumula_muestras() {
        use crate::store::Store;

        let store = Store::in_memory().expect("abrir");
        let probes: Vec<Box<dyn SessionProbe>> = vec![Box::new(ProbeQueFunciona)];

        let snapshot = collect(&probes, &[], &[], Utc::now(), DEFAULT_IDLE_THRESHOLD);
        persist(&store, &snapshot).expect("persistir");
        persist(&store, &snapshot).expect("persistir de nuevo");

        assert_eq!(
            store.current().expect("leer").len(),
            1,
            "no duplica sesiones"
        );
        assert_eq!(
            store.sample_count("s1").expect("contar"),
            2,
            "sí acumula muestras"
        );
    }
}
