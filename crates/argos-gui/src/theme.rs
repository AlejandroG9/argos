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

/// Una antigüedad en segundos crudos ("hace 96423s") es ilegible de un
/// vistazo, que es justo lo que el tablero promete.
pub fn edad_legible(segundos: i64) -> String {
    let s = segundos.max(0);
    match s {
        0..=59 => format!("hace {s}s"),
        60..=3599 => format!("hace {}m", s / 60),
        3600..=86399 => format!("hace {}h", s / 3600),
        _ => format!("hace {}d", s / 86400),
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

    /// "hace 96423s" es lo que mostraba antes: 26 horas en segundos crudos.
    #[test]
    fn la_edad_se_muestra_en_la_unidad_que_se_lee_de_un_vistazo() {
        assert_eq!(edad_legible(5), "hace 5s");
        assert_eq!(edad_legible(59), "hace 59s");
        assert_eq!(edad_legible(60), "hace 1m");
        assert_eq!(edad_legible(3599), "hace 59m");
        assert_eq!(edad_legible(3600), "hace 1h");
        assert_eq!(edad_legible(96423), "hace 1d");
        assert_eq!(edad_legible(310802), "hace 3d");
    }

    #[test]
    fn una_edad_negativa_por_desfase_de_reloj_no_se_muestra_absurda() {
        assert_eq!(edad_legible(-10), "hace 0s");
    }

    #[test]
    fn los_colores_tambien_se_distinguen() {
        let a = state_badge(AgentState::Waiting).1;
        let b = state_badge(AgentState::Working).1;
        assert_ne!(a, b);
    }
}
