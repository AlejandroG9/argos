use crate::error::ProbeError;
use crate::model::ClientKind;
use crate::observation::{ActivitySemantics, Capabilities, SessionObservation};
use crate::probes::SessionProbe;
use crate::scope::Scope;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn parse_rollout(contents: &str, source: &Path) -> Option<SessionObservation> {
    let mut id: Option<String> = None;
    let mut anchor_path: Option<PathBuf> = None;
    let mut first_seen: Option<DateTime<Utc>> = None;
    let mut last_activity: Option<DateTime<Utc>> = None;
    let mut pending_tools: HashSet<String> = HashSet::new();

    for line in contents.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        if let Some(ts) = entry
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        {
            let ts = ts.with_timezone(&Utc);
            first_seen.get_or_insert(ts);
            last_activity = Some(ts);
        }

        if entry.get("type").and_then(Value::as_str) == Some("session_meta")
            && let Some(payload) = entry.get("payload")
        {
            id = payload
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            anchor_path = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(PathBuf::from);
        }

        let Some(payload) = entry.get("payload") else {
            continue;
        };
        let call_id = payload.get("call_id").and_then(Value::as_str);
        match payload.get("type").and_then(Value::as_str) {
            Some("custom_tool_call") => {
                if let Some(call_id) = call_id {
                    pending_tools.insert(call_id.to_string());
                }
            }
            Some("custom_tool_call_output") => {
                if let Some(call_id) = call_id {
                    pending_tools.remove(call_id);
                }
            }
            _ => {}
        }
    }

    let activity = if pending_tools.is_empty() {
        ActivitySemantics::AssistantTurnEnded
    } else {
        ActivitySemantics::ToolCallPending
    };

    Some(SessionObservation {
        id: id?,
        client: ClientKind::Codex,
        anchor_path: anchor_path?,
        git_branch: None,
        first_seen,
        last_activity: last_activity?,
        activity,
        metrics: None,
        parent_id: None,
        source_path: source.to_path_buf(),
    })
}

pub struct CodexProbe {
    root: PathBuf,
}

impl CodexProbe {
    pub fn new(root: PathBuf) -> Self {
        CodexProbe { root }
    }

    pub fn default_root() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".codex/sessions")
    }
}

impl SessionProbe for CodexProbe {
    fn client(&self) -> ClientKind {
        ClientKind::Codex
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tool_level_detail: true,
            token_metrics: false,
            subagents: false,
        }
    }

    fn observe(&self, scope: &Scope) -> Result<Vec<SessionObservation>, ProbeError> {
        if scope.is_empty() {
            return Ok(Vec::new());
        }
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
            let Ok(contents) = std::fs::read_to_string(path) else {
                continue;
            };
            if let Some(observation) = parse_rollout(&contents, path) {
                sessions.push(observation);
            }
        }

        sessions.retain(|s| scope.contains(&s.anchor_path));
        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn leer() -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex/rollout.jsonl"),
        )
        .expect("la muestra debe existir")
    }

    #[test]
    fn toma_el_ancla_y_el_id_del_session_meta() {
        let obs = parse_rollout(&leer(), Path::new("/x/r.jsonl")).expect("debe parsear");

        assert_eq!(obs.id, "01a0597d-489d-7630-b63f-756d20e2efa2");
        assert_eq!(
            obs.anchor_path,
            PathBuf::from("/Users/alex/Proyectos/Orion/.worktrees/adam-slm")
        );
        assert_eq!(obs.client, crate::model::ClientKind::Codex);
    }

    #[test]
    fn la_ultima_actividad_es_el_timestamp_de_la_ultima_entrada() {
        let obs = parse_rollout(&leer(), Path::new("/x/r.jsonl")).expect("debe parsear");
        assert_eq!(
            obs.last_activity.to_rfc3339(),
            "2026-08-31T20:23:45.500+00:00"
        );
    }

    #[test]
    fn sin_session_meta_no_hay_observacion() {
        let sin_meta =
            r#"{"timestamp":"2026-08-31T20:23:10.000Z","type":"event_msg","payload":{}}"#;
        assert!(parse_rollout(sin_meta, Path::new("/x/r.jsonl")).is_none());
    }

    #[test]
    fn una_linea_corrupta_se_salta() {
        let contenido = format!("{}\n{{\"timestamp\":\"2026-08-\n", leer().trim());
        let obs = parse_rollout(&contenido, Path::new("/x/r.jsonl")).expect("debe parsear");
        assert_eq!(
            obs.last_activity.to_rfc3339(),
            "2026-08-31T20:23:45.500+00:00"
        );
    }
}
