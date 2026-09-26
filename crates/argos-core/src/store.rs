use crate::model::{AgentState, ClientKind, Confidence, SessionId, TokenMetrics};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("error de base de datos: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub id: SessionId,
    pub client: ClientKind,
    pub anchor_path: PathBuf,
    pub branch: Option<String>,
    pub warp_focus_url: Option<String>,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_activity: DateTime<Utc>,
    pub state: AgentState,
    pub confidence: Confidence,
    pub parent_id: Option<SessionId>,
    pub depth: u8,
    pub metrics: Option<TokenMetrics>,
}

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id              TEXT PRIMARY KEY,
    client          TEXT NOT NULL,
    anchor_path     TEXT NOT NULL,
    branch          TEXT,
    warp_focus_url  TEXT,
    pid             INTEGER,
    started_at      INTEGER,
    last_activity   INTEGER NOT NULL,
    state           TEXT NOT NULL,
    confidence      TEXT NOT NULL,
    parent_id       TEXT,
    depth           INTEGER NOT NULL DEFAULT 0,
    input_tokens          INTEGER,
    output_tokens         INTEGER,
    cache_read_tokens     INTEGER,
    cache_creation_tokens INTEGER,
    thinking_tokens       INTEGER
);

CREATE TABLE IF NOT EXISTS samples (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT NOT NULL,
    observed_at INTEGER NOT NULL,
    state       TEXT NOT NULL,
    total_tokens INTEGER
);

CREATE INDEX IF NOT EXISTS idx_samples_session ON samples(session_id, observed_at);
"#;

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn upsert_snapshot(&self, rows: &[SessionRow]) -> Result<(), StoreError> {
        for row in rows {
            self.conn.execute(
                "INSERT INTO sessions (
                    id, client, anchor_path, branch, warp_focus_url, pid, started_at,
                    last_activity, state, confidence, parent_id, depth,
                    input_tokens, output_tokens, cache_read_tokens,
                    cache_creation_tokens, thinking_tokens
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
                 ON CONFLICT(id) DO UPDATE SET
                    anchor_path = excluded.anchor_path,
                    branch = excluded.branch,
                    warp_focus_url = excluded.warp_focus_url,
                    pid = excluded.pid,
                    last_activity = excluded.last_activity,
                    state = excluded.state,
                    confidence = excluded.confidence,
                    input_tokens = excluded.input_tokens,
                    output_tokens = excluded.output_tokens,
                    cache_read_tokens = excluded.cache_read_tokens,
                    cache_creation_tokens = excluded.cache_creation_tokens,
                    thinking_tokens = excluded.thinking_tokens",
                params![
                    row.id,
                    client_to_str(row.client),
                    row.anchor_path.to_string_lossy(),
                    row.branch,
                    row.warp_focus_url,
                    row.pid,
                    row.started_at.map(|t| t.timestamp()),
                    row.last_activity.timestamp(),
                    state_to_str(row.state),
                    confidence_to_str(row.confidence),
                    row.parent_id,
                    row.depth,
                    // SQLite solo tiene enteros con signo: los conteos viajan como i64.
                    row.metrics.map(|m| m.input as i64),
                    row.metrics.map(|m| m.output as i64),
                    row.metrics.map(|m| m.cache_read as i64),
                    row.metrics.map(|m| m.cache_creation as i64),
                    row.metrics.map(|m| m.thinking as i64),
                ],
            )?;
        }
        Ok(())
    }

    pub fn record_sample(&self, row: &SessionRow, at: DateTime<Utc>) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO samples (session_id, observed_at, state, total_tokens)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                row.id,
                at.timestamp(),
                state_to_str(row.state),
                row.metrics.map(|m| m.total() as i64),
            ],
        )?;
        Ok(())
    }

    pub fn sample_count(&self, session_id: &str) -> Result<u32, StoreError> {
        let count: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM samples WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    pub fn current(&self) -> Result<Vec<SessionRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, client, anchor_path, branch, warp_focus_url, pid, started_at,
                    last_activity, state, confidence, parent_id, depth,
                    input_tokens, output_tokens, cache_read_tokens,
                    cache_creation_tokens, thinking_tokens
             FROM sessions",
        )?;

        let rows = stmt.query_map([], |r| {
            let leer_conteo = |i: usize| -> u64 {
                r.get::<_, Option<i64>>(i)
                    .ok()
                    .flatten()
                    .unwrap_or(0)
                    .max(0) as u64
            };
            let input: Option<i64> = r.get(12)?;
            let metrics = input.map(|input| TokenMetrics {
                input: input.max(0) as u64,
                output: leer_conteo(13),
                cache_read: leer_conteo(14),
                cache_creation: leer_conteo(15),
                thinking: leer_conteo(16),
            });

            Ok(SessionRow {
                id: r.get(0)?,
                client: client_from_str(&r.get::<_, String>(1)?).unwrap_or(ClientKind::ClaudeCode),
                anchor_path: PathBuf::from(r.get::<_, String>(2)?),
                branch: r.get(3)?,
                warp_focus_url: r.get(4)?,
                pid: r.get(5)?,
                started_at: r
                    .get::<_, Option<i64>>(6)?
                    .and_then(|s| DateTime::from_timestamp(s, 0)),
                last_activity: DateTime::from_timestamp(r.get::<_, i64>(7)?, 0)
                    .unwrap_or_else(Utc::now),
                state: state_from_str(&r.get::<_, String>(8)?).unwrap_or(AgentState::Unknown),
                confidence: confidence_from_str(&r.get::<_, String>(9)?).unwrap_or(Confidence::Low),
                parent_id: r.get(10)?,
                depth: r.get(11)?,
                metrics,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// La base es un índice derivado: vaciarla es una operación normal.
    pub fn reset(&self) -> Result<(), StoreError> {
        self.conn
            .execute_batch("DELETE FROM samples; DELETE FROM sessions;")?;
        Ok(())
    }
}

fn client_to_str(c: ClientKind) -> &'static str {
    match c {
        ClientKind::ClaudeCode => "claude_code",
        ClientKind::Codex => "codex",
        ClientKind::GeminiCli => "gemini_cli",
        ClientKind::Antigravity => "antigravity",
    }
}

fn client_from_str(s: &str) -> Option<ClientKind> {
    ClientKind::ALL.into_iter().find(|c| client_to_str(*c) == s)
}

fn state_to_str(s: AgentState) -> &'static str {
    match s {
        AgentState::Waiting => "waiting",
        AgentState::Working => "working",
        AgentState::Unknown => "unknown",
        AgentState::Finished => "finished",
    }
}

fn state_from_str(s: &str) -> Option<AgentState> {
    match s {
        "waiting" => Some(AgentState::Waiting),
        "working" => Some(AgentState::Working),
        "unknown" => Some(AgentState::Unknown),
        "finished" => Some(AgentState::Finished),
        _ => None,
    }
}

fn confidence_to_str(c: Confidence) -> &'static str {
    match c {
        Confidence::Low => "low",
        Confidence::Medium => "medium",
        Confidence::High => "high",
    }
}

fn confidence_from_str(s: &str) -> Option<Confidence> {
    match s {
        "low" => Some(Confidence::Low),
        "medium" => Some(Confidence::Medium),
        "high" => Some(Confidence::High),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentState, ClientKind, Confidence, TokenMetrics};
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;

    fn fila(id: &str, state: AgentState) -> SessionRow {
        SessionRow {
            id: id.to_string(),
            client: ClientKind::ClaudeCode,
            anchor_path: PathBuf::from("/repo"),
            branch: Some("main".into()),
            warp_focus_url: Some("warp://session/abc".into()),
            pid: Some(42),
            started_at: Some(Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap()),
            last_activity: Utc.with_ymd_and_hms(2026, 9, 26, 12, 5, 0).unwrap(),
            state,
            confidence: Confidence::High,
            parent_id: None,
            depth: 0,
            metrics: Some(TokenMetrics {
                input: 1,
                output: 2,
                cache_read: 3,
                cache_creation: 4,
                thinking: 5,
            }),
        }
    }

    #[test]
    fn guarda_y_recupera_el_estado_actual() {
        let store = Store::in_memory().expect("abrir");
        store
            .upsert_snapshot(&[fila("s1", AgentState::Working)])
            .expect("guardar");

        let filas = store.current().expect("leer");
        assert_eq!(filas.len(), 1);
        assert_eq!(filas[0].id, "s1");
        assert_eq!(filas[0].state, AgentState::Working);
        assert_eq!(filas[0].branch.as_deref(), Some("main"));
        assert_eq!(filas[0].metrics.map(|m| m.thinking), Some(5));
    }

    #[test]
    fn un_upsert_actualiza_en_vez_de_duplicar() {
        let store = Store::in_memory().expect("abrir");
        store
            .upsert_snapshot(&[fila("s1", AgentState::Working)])
            .expect("guardar");
        store
            .upsert_snapshot(&[fila("s1", AgentState::Waiting)])
            .expect("actualizar");

        let filas = store.current().expect("leer");
        assert_eq!(filas.len(), 1, "no debe duplicar la sesión");
        assert_eq!(filas[0].state, AgentState::Waiting);
    }

    #[test]
    fn las_muestras_se_acumulan_para_el_historico() {
        let store = Store::in_memory().expect("abrir");
        let f = fila("s1", AgentState::Working);
        store
            .upsert_snapshot(std::slice::from_ref(&f))
            .expect("guardar");

        store
            .record_sample(&f, Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap())
            .expect("m1");
        store
            .record_sample(&f, Utc.with_ymd_and_hms(2026, 9, 26, 12, 1, 0).unwrap())
            .expect("m2");

        assert_eq!(store.sample_count("s1").expect("contar"), 2);
    }

    /// El spec §7: la base es derivada y debe poder reconstruirse entera.
    #[test]
    fn reset_deja_la_base_vacia_y_reutilizable() {
        let store = Store::in_memory().expect("abrir");
        let f = fila("s1", AgentState::Working);
        store
            .upsert_snapshot(std::slice::from_ref(&f))
            .expect("guardar");
        store.record_sample(&f, Utc::now()).expect("muestra");

        store.reset().expect("reset");

        assert!(store.current().expect("leer").is_empty());
        assert_eq!(store.sample_count("s1").expect("contar"), 0);

        store
            .upsert_snapshot(&[fila("s2", AgentState::Waiting)])
            .expect("reingerir");
        assert_eq!(store.current().expect("leer").len(), 1);
    }

    #[test]
    fn una_sesion_sin_metricas_se_guarda_igual() {
        let store = Store::in_memory().expect("abrir");
        let mut sin = fila("agy", AgentState::Unknown);
        sin.metrics = None;
        sin.pid = None;
        sin.warp_focus_url = None;

        store.upsert_snapshot(&[sin]).expect("guardar");

        let filas = store.current().expect("leer");
        assert_eq!(filas[0].metrics, None);
        assert_eq!(filas[0].pid, None);
    }
}
