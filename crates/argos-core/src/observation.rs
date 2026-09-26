use crate::model::{ClientKind, SessionId, TokenMetrics};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Un proceso de agente vivo, visto en la tabla de procesos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessObservation {
    pub pid: u32,
    pub ppid: u32,
    pub client: ClientKind,
    /// `None` si `lsof` no pudo resolverlo (proceso murió entre el listado y la consulta).
    pub cwd: Option<PathBuf>,
    pub started_at: DateTime<Utc>,
    pub warp_session_uuid: Option<String>,
    pub warp_focus_url: Option<String>,
}

impl ProcessObservation {
    /// Un agente corrido fuera de Warp existe igual, solo que sin salto.
    pub fn can_jump(&self) -> bool {
        self.warp_focus_url.is_some()
    }
}

/// Qué está haciendo la sesión según la última entrada de su archivo.
/// Esta es la señal decisiva del motor de estados: es semántica, no temporal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivitySemantics {
    /// Hay un `tool_use` sin su `tool_result`. El agente está ejecutando algo,
    /// por más tiempo que lleve quieto el archivo (ej. un build de 5 minutos).
    ToolCallPending,
    /// El asistente cerró turno. Está esperando al usuario.
    AssistantTurnEnded,
    /// La plataforma no expone detalle suficiente para distinguir.
    Indeterminate,
}

/// Una sesión de agente leída de su archivo en disco.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionObservation {
    pub id: SessionId,
    pub client: ClientKind,
    /// Ruta que ancla la sesión a un worktree. Es la llave de correlación.
    pub anchor_path: PathBuf,
    /// Solo Claude Code la registra directamente; para el resto se deriva.
    pub git_branch: Option<String>,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_activity: DateTime<Utc>,
    pub activity: ActivitySemantics,
    pub metrics: Option<TokenMetrics>,
    /// `Some` solo para subagentes.
    pub parent_id: Option<SessionId>,
    pub source_path: PathBuf,
}

/// Lo que un probe puede aportar. La UI muestra lo conocido y marca lo que no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub tool_level_detail: bool,
    pub token_metrics: bool,
    pub subagents: bool,
}

impl Capabilities {
    pub fn full() -> Self {
        Capabilities {
            tool_level_detail: true,
            token_metrics: true,
            subagents: true,
        }
    }

    pub fn minimal() -> Self {
        Capabilities {
            tool_level_detail: false,
            token_metrics: false,
            subagents: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ClientKind;
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn una_observacion_de_proceso_sin_warp_no_ofrece_salto() {
        let sin_warp = ProcessObservation {
            pid: 100,
            ppid: 1,
            client: ClientKind::ClaudeCode,
            cwd: Some(PathBuf::from("/tmp/proyecto")),
            started_at: Utc::now(),
            warp_session_uuid: None,
            warp_focus_url: None,
        };
        assert!(!sin_warp.can_jump());

        let con_warp = ProcessObservation {
            warp_focus_url: Some("warp://session/abc".to_string()),
            ..sin_warp.clone()
        };
        assert!(con_warp.can_jump());
    }

    #[test]
    fn las_capacidades_declaran_si_la_plataforma_distingue_trabajando_de_esperando() {
        assert!(Capabilities::full().tool_level_detail);
        assert!(!Capabilities::minimal().tool_level_detail);
        assert!(!Capabilities::minimal().token_metrics);
    }
}
