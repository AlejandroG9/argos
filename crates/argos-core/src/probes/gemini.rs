use crate::error::ProbeError;
use crate::model::ClientKind;
use crate::observation::{ActivitySemantics, Capabilities, SessionObservation};
use crate::probes::SessionProbe;
use crate::scope::Scope;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn read_project_root(project_dir: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(project_dir.join(".project_root")).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

pub fn parse_chat(contents: &str, anchor: &Path, source: &Path) -> Option<SessionObservation> {
    if contents.trim().is_empty() {
        return None;
    }

    let id = source.file_stem()?.to_str()?.to_string();
    let mut timestamps = Vec::new();

    for line in contents.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        collect_timestamps(&entry, &mut timestamps);
    }

    let first_seen = timestamps.iter().min().cloned();
    let last_activity = timestamps
        .iter()
        .max()
        .cloned()
        .or_else(|| mtime_of(source))
        .unwrap_or_else(Utc::now);

    Some(SessionObservation {
        id,
        client: ClientKind::GeminiCli,
        anchor_path: anchor.to_path_buf(),
        git_branch: None,
        first_seen,
        last_activity,
        activity: ActivitySemantics::Indeterminate,
        metrics: None,
        parent_id: None,
        source_path: source.to_path_buf(),
    })
}

fn collect_timestamps(value: &Value, timestamps: &mut Vec<DateTime<Utc>>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if matches!(key.as_str(), "startTime" | "lastUpdated" | "timestamp")
                    && let Some(timestamp) = value
                        .as_str()
                        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                {
                    timestamps.push(timestamp.with_timezone(&Utc));
                }
                collect_timestamps(value, timestamps);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_timestamps(value, timestamps);
            }
        }
        _ => {}
    }
}

fn mtime_of(path: &Path) -> Option<DateTime<Utc>> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(DateTime::<Utc>::from(modified))
}

pub struct GeminiProbe {
    root: PathBuf,
}

impl GeminiProbe {
    pub fn new(root: PathBuf) -> Self {
        GeminiProbe { root }
    }

    pub fn default_root() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".gemini/tmp")
    }
}

impl SessionProbe for GeminiProbe {
    fn client(&self) -> ClientKind {
        ClientKind::GeminiCli
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tool_level_detail: false,
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

        let Ok(projects) = std::fs::read_dir(&self.root) else {
            return Ok(sessions);
        };

        for project in projects.filter_map(Result::ok) {
            let project_dir = project.path();
            let Some(anchor) = read_project_root(&project_dir) else {
                continue;
            };

            let Ok(chats) = std::fs::read_dir(project_dir.join("chats")) else {
                continue;
            };

            for chat in chats.filter_map(Result::ok) {
                let path = chat.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(contents) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(observation) = parse_chat(&contents, &anchor, &path) {
                    sessions.push(observation);
                }
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

    #[test]
    fn el_ancla_viene_del_project_root_no_del_chat() {
        let dir = tempdir("con-project-root");
        let proyecto = dir.join("orion");
        std::fs::create_dir_all(proyecto.join("chats")).expect("crear dirs");
        std::fs::write(
            proyecto.join(".project_root"),
            "/Users/alex/Proyectos/Orion\n",
        )
        .expect("escribir project_root");

        assert_eq!(
            read_project_root(&proyecto),
            Some(PathBuf::from("/Users/alex/Proyectos/Orion"))
        );
    }

    #[test]
    fn sin_project_root_no_se_puede_anclar() {
        let dir = tempdir("sin-project-root");
        std::fs::create_dir_all(&dir).expect("crear dir");
        assert_eq!(read_project_root(&dir), None);
    }

    #[test]
    fn el_id_es_el_nombre_del_archivo_de_sesion() {
        let obs = parse_chat(
            "{\"role\":\"user\",\"parts\":[]}\n",
            Path::new("/Users/alex/Proyectos/Orion"),
            Path::new("/x/chats/session-2026-09-03T18-11-db47e457.jsonl"),
        )
        .expect("debe parsear");

        assert_eq!(obs.id, "session-2026-09-03T18-11-db47e457");
        assert_eq!(obs.client, crate::model::ClientKind::GeminiCli);
        assert_eq!(
            obs.anchor_path,
            PathBuf::from("/Users/alex/Proyectos/Orion")
        );
    }

    /// El nombre lo da quien llama: los tests corren en paralelo y un
    /// directorio compartido se borraría bajo los pies del otro test.
    fn tempdir(nombre: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("argos-gemini-{}-{nombre}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("crear tempdir");
        p
    }
}
