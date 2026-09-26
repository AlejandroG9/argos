use argos_core::model::{AgentState, Confidence};
use egui::Color32;

/// Símbolo y color por estado. El símbolo es obligatorio: el color solo
/// refuerza, nunca es la única señal (spec §8).
pub fn state_badge(state: AgentState) -> (&'static str, Color32) {
    match state {
        AgentState::Waiting => ("◆", Color32::from_rgb(230, 160, 30)),
        AgentState::Working => ("▶", Color32::from_rgb(60, 170, 110)),
        AgentState::Unknown => ("?", Color32::from_rgb(140, 140, 150)),
        AgentState::Finished => ("✓", Color32::from_rgb(95, 110, 130)),
    }
}

pub fn state_label(state: AgentState) -> &'static str {
    match state {
        AgentState::Waiting => "esperando respuesta",
        AgentState::Working => "trabajando",
        AgentState::Unknown => "desconocido",
        AgentState::Finished => "terminó",
    }
}

/// Una inferencia dudosa se muestra como dudosa (spec §4).
pub fn confidence_hint(confidence: Confidence) -> Option<&'static str> {
    match confidence {
        Confidence::High => None,
        Confidence::Medium => Some("~"),
        Confidence::Low => Some("?"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argos_core::model::AgentState;

    /// El spec §8 pide color **y** forma: un indicador que dependa solo del
    /// color es inservible para quien no distingue esos colores.
    #[test]
    fn cada_estado_tiene_simbolo_propio_ademas_de_color() {
        let simbolos: Vec<&str> = [
            AgentState::Waiting,
            AgentState::Working,
            AgentState::Unknown,
            AgentState::Finished,
        ]
        .into_iter()
        .map(|s| state_badge(s).0)
        .collect();

        let unicos: std::collections::HashSet<_> = simbolos.iter().collect();
        assert_eq!(unicos.len(), 4, "los símbolos deben distinguirse entre sí");
    }

    #[test]
    fn los_colores_tambien_se_distinguen() {
        let a = state_badge(AgentState::Waiting).1;
        let b = state_badge(AgentState::Working).1;
        assert_ne!(a, b);
    }
}
