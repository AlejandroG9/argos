use serde::{Deserialize, Serialize};

/// La compañía detrás del agente. Es lo que el usuario quiere ver de un vistazo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Vendor {
    Anthropic,
    OpenAI,
    Google,
}

/// El CLI concreto. Google tiene dos con formatos en disco incompatibles,
/// por eso vendor y cliente son campos distintos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClientKind {
    ClaudeCode,
    Codex,
    GeminiCli,
    Antigravity,
}

impl ClientKind {
    pub const ALL: [ClientKind; 4] = [
        ClientKind::ClaudeCode,
        ClientKind::Codex,
        ClientKind::GeminiCli,
        ClientKind::Antigravity,
    ];

    pub fn vendor(self) -> Vendor {
        match self {
            ClientKind::ClaudeCode => Vendor::Anthropic,
            ClientKind::Codex => Vendor::OpenAI,
            ClientKind::GeminiCli | ClientKind::Antigravity => Vendor::Google,
        }
    }

    /// Nombre del ejecutable tal como aparece en la tabla de procesos.
    pub fn process_name(self) -> &'static str {
        match self {
            ClientKind::ClaudeCode => "claude",
            ClientKind::Codex => "codex",
            ClientKind::GeminiCli => "gemini",
            ClientKind::Antigravity => "agy",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ClientKind::ClaudeCode => "Claude Code",
            ClientKind::Codex => "Codex",
            ClientKind::GeminiCli => "Gemini",
            ClientKind::Antigravity => "Antigravity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentState {
    /// Bloqueado esperando al usuario. Lo más urgente.
    Waiting,
    Working,
    /// Hay proceso pero no se pudo determinar qué hace. Estado legítimo:
    /// es preferible declarar ignorancia a afirmar algo falso.
    Unknown,
    Finished,
}

impl AgentState {
    /// Orden de presentación: lo que reclama atención del usuario primero.
    pub fn urgency(self) -> u8 {
        match self {
            AgentState::Waiting => 0,
            AgentState::Working => 1,
            AgentState::Unknown => 2,
            AgentState::Finished => 3,
        }
    }
}

/// Qué tan seguro está Argos de una inferencia. Se muestra en la UI:
/// una correlación dudosa se presenta como dudosa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenMetrics {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub thinking: u64,
}

impl TokenMetrics {
    /// `thinking` es un subconjunto de `output`, así que no se suma aparte.
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_creation
    }
}

/// Identificador estable de sesión. Para subagentes de Claude Code es el
/// nombre del archivo `agent-<id>`; para el resto, el id que da la plataforma.
pub type SessionId = String;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_cliente_conoce_su_vendor_y_su_proceso() {
        assert_eq!(ClientKind::ClaudeCode.vendor(), Vendor::Anthropic);
        assert_eq!(ClientKind::Codex.vendor(), Vendor::OpenAI);
        assert_eq!(ClientKind::GeminiCli.vendor(), Vendor::Google);
        assert_eq!(ClientKind::Antigravity.vendor(), Vendor::Google);

        assert_eq!(ClientKind::ClaudeCode.process_name(), "claude");
        assert_eq!(ClientKind::Codex.process_name(), "codex");
        assert_eq!(ClientKind::GeminiCli.process_name(), "gemini");
        assert_eq!(ClientKind::Antigravity.process_name(), "agy");
    }

    #[test]
    fn los_estados_se_ordenan_por_urgencia() {
        let mut estados = vec![
            AgentState::Finished,
            AgentState::Working,
            AgentState::Unknown,
            AgentState::Waiting,
        ];
        estados.sort_by_key(|e| e.urgency());

        assert_eq!(
            estados,
            vec![
                AgentState::Waiting,
                AgentState::Working,
                AgentState::Unknown,
                AgentState::Finished,
            ]
        );
    }

    #[test]
    fn las_metricas_de_tokens_suman_el_total_facturable() {
        let m = TokenMetrics {
            input: 100,
            output: 50,
            cache_read: 1000,
            cache_creation: 200,
            thinking: 30,
        };
        // thinking ya viene incluido dentro de output: no se suma dos veces.
        assert_eq!(m.total(), 1350);
    }
}
