use crate::error::ProbeError;
use crate::model::ClientKind;
use crate::observation::{ActivitySemantics, Capabilities, SessionObservation};
use crate::probes::SessionProbe;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// El historial es un archivo global con una línea por prompt. Cada
/// `workspace` distinto se trata como una sesión lógica.
pub fn parse_history(contents: &str, source: &Path) -> Vec<SessionObservation> {
    let mut por_workspace: BTreeMap<String, (i64, i64)> = BTreeMap::new();

    for line in contents.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(workspace) = entry.get("workspace").and_then(Value::as_str) else {
            continue;
        };
        let Some(millis) = entry.get("timestamp").and_then(Value::as_i64) else {
            continue;
        };

        por_workspace
            .entry(workspace.to_string())
            .and_modify(|(first, last)| {
                *first = (*first).min(millis);
                *last = (*last).max(millis);
            })
            .or_insert((millis, millis));
    }

    por_workspace
        .into_iter()
        .filter_map(|(workspace, (first, last))| {
            Some(SessionObservation {
                id: format!("agy:{workspace}"),
                client: ClientKind::Antigravity,
                anchor_path: PathBuf::from(&workspace),
                git_branch: None,
                first_seen: from_millis(first),
                last_activity: from_millis(last)?,
                activity: ActivitySemantics::Indeterminate,
                metrics: None,
                parent_id: None,
                source_path: source.to_path_buf(),
            })
        })
        .collect()
}

/// Antigravity usa epoch en **milisegundos**, a diferencia del resto.
fn from_millis(millis: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(millis)
}

pub struct AntigravityProbe {
    history: PathBuf,
}

impl AntigravityProbe {
    pub fn new(history: PathBuf) -> Self {
        AntigravityProbe { history }
    }

    pub fn default_history() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".gemini/antigravity-cli/history.jsonl")
    }
}

impl SessionProbe for AntigravityProbe {
    fn client(&self) -> ClientKind {
        ClientKind::Antigravity
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::minimal()
    }

    fn observe(&self) -> Result<Vec<SessionObservation>, ProbeError> {
        if !self.history.exists() {
            return Err(ProbeError::SourceMissing(self.history.clone()));
        }

        let contents = std::fs::read_to_string(&self.history).map_err(|source| ProbeError::Io {
            path: self.history.clone(),
            source,
        })?;

        Ok(parse_history(&contents, &self.history))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::ActivitySemantics;
    use std::path::{Path, PathBuf};

    fn leer() -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/antigravity/history.jsonl"),
        )
        .expect("la muestra debe existir")
    }

    #[test]
    fn agrupa_el_historial_global_por_workspace() {
        let sesiones = parse_history(&leer(), Path::new("/x/history.jsonl"));
        assert_eq!(sesiones.len(), 2, "dos workspaces distintos");

        let anclas: Vec<PathBuf> = sesiones.iter().map(|s| s.anchor_path.clone()).collect();
        assert!(anclas.contains(&PathBuf::from("/Users/alex/Proyectos/Laboratorio")));
        assert!(anclas.contains(&PathBuf::from("/Users/alex/Proyectos/Orion")));
    }

    /// Review Focus #2: el timestamp viene en milisegundos, no en segundos
    /// ni en ISO-8601. Tratarlo como segundos daría una fecha en 1970.
    #[test]
    fn interpreta_el_timestamp_como_epoch_en_milisegundos() {
        let sesiones = parse_history(&leer(), Path::new("/x/history.jsonl"));
        let laboratorio = sesiones
            .iter()
            .find(|s| s.anchor_path.ends_with("Laboratorio"))
            .expect("debe existir");

        // 1782514800000 ms = 2026-06-27T... — debe caer en 2026, no en 1970.
        assert_eq!(laboratorio.last_activity.format("%Y").to_string(), "2026");
        assert_eq!(laboratorio.last_activity.timestamp_millis(), 1782514800000);
        assert_eq!(
            laboratorio
                .first_seen
                .expect("hay primera entrada")
                .timestamp_millis(),
            1782514696039
        );
    }

    #[test]
    fn antigravity_no_distingue_trabajando_de_esperando() {
        let sesiones = parse_history(&leer(), Path::new("/x/history.jsonl"));
        assert!(
            sesiones
                .iter()
                .all(|s| s.activity == ActivitySemantics::Indeterminate)
        );
        assert!(sesiones.iter().all(|s| s.metrics.is_none()));
    }

    #[test]
    fn una_entrada_sin_workspace_se_ignora() {
        let contenido = "{\"display\":\"x\",\"timestamp\":1782514696039}\n";
        assert!(parse_history(contenido, Path::new("/x/history.jsonl")).is_empty());
    }
}
