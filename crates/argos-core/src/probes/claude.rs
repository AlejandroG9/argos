use crate::error::ProbeError;
use crate::model::{ClientKind, SessionId, TokenMetrics};
use crate::observation::{ActivitySemantics, Capabilities, SessionObservation};
use crate::probes::SessionProbe;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub struct SessionIdentity {
    pub id: SessionId,
    pub parent_id: Option<SessionId>,
}

/// Claude Code codifica la relación padre-hijo en la ruta:
/// `<slug>/<sesión-uuid>/subagents/agent-<id>.jsonl`.
pub fn identity_from_path(path: &Path) -> Option<SessionIdentity> {
    let stem = path.file_stem()?.to_str()?.to_string();

    let is_subagent = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        == Some("subagents");

    if !is_subagent {
        return Some(SessionIdentity {
            id: stem,
            parent_id: None,
        });
    }

    let parent_id = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(str::to_string);

    Some(SessionIdentity {
        id: stem,
        parent_id,
    })
}

/// Recorre el archivo línea por línea acumulando estado. Las líneas que no
/// parsean se saltan: el agente puede estar escribiendo mientras leemos.
pub fn parse_session(
    contents: &str,
    source: &Path,
    id: SessionId,
    parent_id: Option<SessionId>,
) -> Option<SessionObservation> {
    let mut anchor_path: Option<PathBuf> = None;
    let mut git_branch: Option<String> = None;
    let mut first_seen: Option<DateTime<Utc>> = None;
    let mut last_activity: Option<DateTime<Utc>> = None;
    let mut metrics = TokenMetrics::default();
    let mut saw_metrics = false;
    let mut pending_tools: Vec<String> = Vec::new();
    let mut last_role: Option<String> = None;

    for line in contents.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        if let Some(cwd) = entry.get("cwd").and_then(Value::as_str) {
            anchor_path = Some(PathBuf::from(cwd));
        }
        if let Some(branch) = entry.get("gitBranch").and_then(Value::as_str) {
            git_branch = Some(branch.to_string());
        }
        if let Some(ts) = entry
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        {
            let ts = ts.with_timezone(&Utc);
            first_seen.get_or_insert(ts);
            last_activity = Some(ts);
        }

        let Some(message) = entry.get("message") else {
            continue;
        };

        if let Some(role) = message.get("role").and_then(Value::as_str) {
            last_role = Some(role.to_string());
        }

        if let Some(usage) = message.get("usage") {
            saw_metrics = true;
            metrics.input += field(usage, "input_tokens");
            metrics.output += field(usage, "output_tokens");
            metrics.cache_read += field(usage, "cache_read_input_tokens");
            metrics.cache_creation += field(usage, "cache_creation_input_tokens");
            metrics.thinking += usage
                .get("output_tokens_details")
                .map(|d| field(d, "thinking_tokens"))
                .unwrap_or(0);
        }

        if let Some(blocks) = message.get("content").and_then(Value::as_array) {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        if let Some(tool_id) = block.get("id").and_then(Value::as_str) {
                            pending_tools.push(tool_id.to_string());
                        }
                    }
                    Some("tool_result") => {
                        if let Some(tool_id) = block.get("tool_use_id").and_then(Value::as_str) {
                            pending_tools.retain(|p| p != tool_id);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let anchor_path = anchor_path?;
    let last_activity = last_activity?;

    // La regla de desempate del spec §5: la semántica gana sobre el tiempo.
    let activity = if !pending_tools.is_empty() {
        ActivitySemantics::ToolCallPending
    } else if last_role.as_deref() == Some("assistant") {
        ActivitySemantics::AssistantTurnEnded
    } else {
        ActivitySemantics::Indeterminate
    };

    Some(SessionObservation {
        id,
        client: ClientKind::ClaudeCode,
        anchor_path,
        git_branch,
        first_seen,
        last_activity,
        activity,
        metrics: saw_metrics.then_some(metrics),
        parent_id,
        source_path: source.to_path_buf(),
    })
}

fn field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub struct ClaudeProbe {
    root: PathBuf,
}

impl ClaudeProbe {
    pub fn new(root: PathBuf) -> Self {
        ClaudeProbe { root }
    }

    pub fn default_root() -> PathBuf {
        home().join(".claude/projects")
    }
}

impl SessionProbe for ClaudeProbe {
    fn client(&self) -> ClientKind {
        ClientKind::ClaudeCode
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::full()
    }

    fn observe(&self) -> Result<Vec<SessionObservation>, ProbeError> {
        if !self.root.exists() {
            return Err(ProbeError::SourceMissing(self.root.clone()));
        }

        let mut sessions = Vec::new();

        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(identity) = identity_from_path(path) else {
                continue;
            };
            let Ok(contents) = std::fs::read_to_string(path) else {
                continue;
            };
            if let Some(observation) =
                parse_session(&contents, path, identity.id, identity.parent_id)
            {
                sessions.push(observation);
            }
        }

        Ok(sessions)
    }
}

fn home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::ActivitySemantics;
    use std::path::Path;

    fn leer(nombre: &str) -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/claude")
                .join(nombre),
        )
        .expect("la muestra debe existir")
    }

    #[test]
    fn una_sesion_con_turno_cerrado_esta_esperando() {
        let obs = parse_session(
            &leer("waiting.jsonl"),
            Path::new("/x/s.jsonl"),
            "s".into(),
            None,
        )
        .expect("debe parsear");

        assert_eq!(obs.activity, ActivitySemantics::AssistantTurnEnded);
        assert_eq!(
            obs.anchor_path,
            PathBuf::from("/Users/alex/Proyectos/Orion")
        );
        assert_eq!(obs.git_branch.as_deref(), Some("main"));
    }

    #[test]
    fn una_sesion_con_herramienta_pendiente_esta_trabajando() {
        let obs = parse_session(
            &leer("working.jsonl"),
            Path::new("/x/s.jsonl"),
            "s".into(),
            None,
        )
        .expect("debe parsear");

        assert_eq!(obs.activity, ActivitySemantics::ToolCallPending);
        assert_eq!(obs.git_branch.as_deref(), Some("feat/slm-multi-lora"));
        assert_eq!(
            obs.anchor_path,
            PathBuf::from("/Users/alex/Proyectos/Orion/.worktrees/adam-slm")
        );
    }

    #[test]
    fn acumula_los_tokens_de_todos_los_turnos() {
        let obs = parse_session(
            &leer("waiting.jsonl"),
            Path::new("/x/s.jsonl"),
            "s".into(),
            None,
        )
        .expect("debe parsear");
        let m = obs.metrics.expect("Claude Code sí expone tokens");

        assert_eq!(m.input, 10);
        assert_eq!(m.output, 20);
        assert_eq!(m.cache_read, 100);
        assert_eq!(m.cache_creation, 50);
        assert_eq!(m.thinking, 5);
    }

    /// Review Focus #1: el agente escribe mientras Argos lee.
    #[test]
    fn una_linea_corrupta_se_salta_sin_perder_la_sesion() {
        let obs = parse_session(
            &leer("corrupto.jsonl"),
            Path::new("/x/s.jsonl"),
            "s".into(),
            None,
        )
        .expect("una línea rota no debe descartar la sesión entera");

        assert_eq!(obs.activity, ActivitySemantics::AssistantTurnEnded);
        assert_eq!(obs.git_branch.as_deref(), Some("main"));
        // La última entrada válida es la tercera, de las 12:00:09.
        assert_eq!(obs.last_activity.to_rfc3339(), "2026-09-26T12:00:09+00:00");
    }

    #[test]
    fn un_archivo_vacio_no_produce_observacion() {
        assert!(parse_session("", Path::new("/x/s.jsonl"), "s".into(), None).is_none());
    }

    #[test]
    fn reconoce_un_subagente_por_su_ruta() {
        let ruta = Path::new(
            "/Users/alex/.claude/projects/-Users-alex-Proyectos-Orion/abc-123/subagents/agent-def456.jsonl",
        );
        let identidad = identity_from_path(ruta).expect("debe reconocer el subagente");

        assert_eq!(identidad.id, "agent-def456");
        assert_eq!(identidad.parent_id.as_deref(), Some("abc-123"));
    }

    #[test]
    fn reconoce_una_sesion_raiz_por_su_ruta() {
        let ruta =
            Path::new("/Users/alex/.claude/projects/-Users-alex-Proyectos-Orion/abc-123.jsonl");
        let identidad = identity_from_path(ruta).expect("debe reconocer la sesión");

        assert_eq!(identidad.id, "abc-123");
        assert_eq!(identidad.parent_id, None);
    }

    /// Se salta si la máquina no tiene Claude Code instalado.
    #[test]
    fn lee_las_sesiones_reales_de_la_maquina() {
        let root = ClaudeProbe::default_root();
        if !root.exists() {
            return;
        }

        let probe = ClaudeProbe::new(root);
        let sesiones = probe.observe().expect("debe recolectar");

        for s in &sesiones {
            assert!(s.anchor_path.is_absolute(), "ancla relativa: {s:?}");
        }
    }
}
