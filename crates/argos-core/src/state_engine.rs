use crate::correlator::Correlated;
use crate::model::{AgentState, Confidence};
use crate::observation::ActivitySemantics;
use chrono::{DateTime, Duration, Utc};

/// Cuánto silencio hace falta para dejar de asumir que una plataforma sin
/// detalle de herramienta sigue trabajando.
pub const DEFAULT_IDLE_THRESHOLD: Duration = Duration::seconds(30);

/// Lógica pura: no lee disco, ni procesos, ni el reloj del sistema.
pub fn infer(
    correlated: &Correlated,
    now: DateTime<Utc>,
    idle_threshold: Duration,
) -> (AgentState, Confidence) {
    let Some(_process) = &correlated.process else {
        // Sin proceso vivo la sesión terminó, sin importar qué tan reciente
        // sea la última actividad ni qué dijera la última entrada.
        return (AgentState::Finished, Confidence::High);
    };

    let idle = now - correlated.session.last_activity;

    // Un subagente no habla contigo: cuando cierra su turno ha terminado y
    // devuelve el control a su padre. Sin esto quedaba "esperando respuesta"
    // para siempre, heredando además el proceso vivo de su padre.
    let es_subagente = correlated.session.parent_id.is_some();

    let (state, own_confidence) = match correlated.session.activity {
        // La señal decisiva es semántica, no temporal.
        ActivitySemantics::ToolCallPending => (AgentState::Working, Confidence::High),
        ActivitySemantics::AssistantTurnEnded if es_subagente => {
            (AgentState::Finished, Confidence::High)
        }
        ActivitySemantics::AssistantTurnEnded => (AgentState::Waiting, Confidence::High),
        // El turno es suyo y hay proceso vivo: está contestando, por mucho
        // que el archivo lleve rato quieto. Un modelo pensando no escribe.
        ActivitySemantics::UserTurnEnded => (AgentState::Working, Confidence::High),
        ActivitySemantics::Indeterminate if idle < idle_threshold => {
            (AgentState::Working, Confidence::Low)
        }
        ActivitySemantics::Indeterminate => (AgentState::Unknown, Confidence::Low),
    };

    // No se puede estar más seguro del estado que de la correlación que lo sostiene.
    (state, own_confidence.min(correlated.confidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::Worktree;
    use crate::model::{AgentState, ClientKind, Confidence};
    use crate::observation::{ActivitySemantics, ProcessObservation, SessionObservation};
    use chrono::{Duration, TimeZone};
    use std::path::PathBuf;

    fn t(offset: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap() + Duration::seconds(offset)
    }

    /// El caso que hizo desaparecer Orion del tablero: una herramienta
    /// devolvió su resultado, el asistente aún no ha contestado, y el archivo
    /// lleva más de medio minuto quieto porque el modelo está pensando.
    /// Antes caía en Indeterminate y de ahí a Unknown, que el filtro
    /// "activas" esconde: la sesión existía, el proceso estaba vivo, y aun
    /// así el tablero decía que no pasaba nada.
    #[test]
    fn si_el_ultimo_turno_es_del_usuario_el_agente_debe_una_respuesta() {
        let c = caso(ActivitySemantics::UserTurnEnded, true, 0);

        // 300 s de silencio: el modelo puede tardar. El tiempo no manda
        // cuando la semántica ya dice de quién es el turno.
        let (estado, confianza) = infer(&c, t(300), DEFAULT_IDLE_THRESHOLD);

        assert_eq!(estado, AgentState::Working);
        assert_eq!(confianza, Confidence::High);
    }

    /// Un subagente que recibe un resultado de herramienta tampoco terminó:
    /// le toca contestar a él, igual que a uno raíz.
    #[test]
    fn un_subagente_con_el_turno_del_usuario_abierto_sigue_trabajando() {
        let mut c = caso(ActivitySemantics::UserTurnEnded, true, 0);
        c.session.parent_id = Some("padre".into());

        assert_eq!(
            infer(&c, t(300), DEFAULT_IDLE_THRESHOLD).0,
            AgentState::Working
        );
    }

    /// Y sin proceso vivo manda el proceso: por muy abierto que quedara el
    /// turno, nadie va a contestarlo.
    #[test]
    fn el_turno_del_usuario_abierto_no_revive_una_sesion_muerta() {
        let c = caso(ActivitySemantics::UserTurnEnded, false, 0);

        assert_eq!(
            infer(&c, t(300), DEFAULT_IDLE_THRESHOLD).0,
            AgentState::Finished
        );
    }

    fn caso(activity: ActivitySemantics, vivo: bool, ultima_actividad: i64) -> Correlated {
        Correlated {
            session: SessionObservation {
                id: "s".into(),
                client: ClientKind::ClaudeCode,
                anchor_path: PathBuf::from("/repo"),
                git_branch: Some("main".into()),
                first_seen: Some(t(0)),
                last_activity: t(ultima_actividad),
                activity,
                metrics: None,
                parent_id: None,
                source_path: PathBuf::from("/logs/s.jsonl"),
            },
            process: vivo.then(|| ProcessObservation {
                pid: 1,
                ppid: 0,
                client: ClientKind::ClaudeCode,
                cwd: Some(PathBuf::from("/repo")),
                started_at: t(-10),
                warp_session_uuid: None,
                warp_focus_url: None,
            }),
            worktree: Some(Worktree {
                path: PathBuf::from("/repo"),
                branch: Some("main".into()),
                repo_root: PathBuf::from("/repo"),
            }),
            confidence: Confidence::High,
        }
    }

    const UMBRAL: Duration = Duration::seconds(30);

    #[test]
    fn sin_proceso_vivo_la_sesion_termino() {
        let (estado, confianza) = infer(
            &caso(ActivitySemantics::AssistantTurnEnded, false, 0),
            t(100),
            UMBRAL,
        );
        assert_eq!(estado, AgentState::Finished);
        assert_eq!(confianza, Confidence::High);
    }

    /// Review Focus #4: el agente crasheó hace dos segundos. No está trabajando.
    #[test]
    fn un_agente_recien_muerto_termino_no_trabaja() {
        let (estado, _) = infer(
            &caso(ActivitySemantics::ToolCallPending, false, 98),
            t(100),
            UMBRAL,
        );
        assert_eq!(
            estado,
            AgentState::Finished,
            "sin proceso no puede estar trabajando, por reciente que sea la actividad"
        );
    }

    /// La regla de desempate del spec §5: la semántica gana sobre el tiempo.
    #[test]
    fn una_herramienta_pendiente_es_trabajo_aunque_el_archivo_lleve_horas_quieto() {
        let (estado, confianza) = infer(
            &caso(ActivitySemantics::ToolCallPending, true, 0),
            t(7200),
            UMBRAL,
        );
        assert_eq!(estado, AgentState::Working);
        assert_eq!(confianza, Confidence::High);
    }

    #[test]
    fn un_turno_cerrado_es_espera_al_usuario() {
        let (estado, confianza) = infer(
            &caso(ActivitySemantics::AssistantTurnEnded, true, 90),
            t(100),
            UMBRAL,
        );
        assert_eq!(estado, AgentState::Waiting);
        assert_eq!(confianza, Confidence::High);
    }

    #[test]
    fn sin_detalle_de_herramienta_la_actividad_reciente_se_asume_trabajo_con_baja_confianza() {
        let (estado, confianza) = infer(
            &caso(ActivitySemantics::Indeterminate, true, 95),
            t(100),
            UMBRAL,
        );
        assert_eq!(estado, AgentState::Working);
        assert_eq!(confianza, Confidence::Low);
    }

    #[test]
    fn sin_detalle_de_herramienta_y_sin_actividad_el_estado_es_desconocido() {
        let (estado, confianza) = infer(
            &caso(ActivitySemantics::Indeterminate, true, 0),
            t(100),
            UMBRAL,
        );
        assert_eq!(estado, AgentState::Unknown);
        assert_eq!(confianza, Confidence::Low);
    }

    /// Un subagente no habla contigo: termina y devuelve el control a su
    /// padre. Leer su turno cerrado como "esperando respuesta" lo deja
    /// eternamente vivo — 26 subagentes de hace tres días aparecían
    /// esperándote, heredando el proceso de una sesión ajena.
    #[test]
    fn un_subagente_con_el_turno_cerrado_termino_no_espera() {
        let mut sub = caso(ActivitySemantics::AssistantTurnEnded, true, 0);
        sub.session.parent_id = Some("padre".into());

        let (estado, _) = infer(&sub, t(100), UMBRAL);
        assert_eq!(estado, AgentState::Finished);
    }

    #[test]
    fn una_sesion_raiz_con_el_turno_cerrado_si_te_espera() {
        let raiz = caso(ActivitySemantics::AssistantTurnEnded, true, 0);
        assert!(raiz.session.parent_id.is_none());

        let (estado, _) = infer(&raiz, t(100), UMBRAL);
        assert_eq!(estado, AgentState::Waiting);
    }

    /// Un subagente a mitad de una herramienta sí sigue trabajando.
    #[test]
    fn un_subagente_con_herramienta_pendiente_sigue_trabajando() {
        let mut sub = caso(ActivitySemantics::ToolCallPending, true, 0);
        sub.session.parent_id = Some("padre".into());

        let (estado, _) = infer(&sub, t(10), UMBRAL);
        assert_eq!(estado, AgentState::Working);
    }

    #[test]
    fn la_confianza_de_la_correlacion_limita_la_confianza_del_estado() {
        let mut dudoso = caso(ActivitySemantics::ToolCallPending, true, 0);
        dudoso.confidence = Confidence::Medium;

        let (estado, confianza) = infer(&dudoso, t(10), UMBRAL);
        assert_eq!(estado, AgentState::Working);
        assert_eq!(
            confianza,
            Confidence::Medium,
            "no se puede estar más seguro del estado que de a qué proceso pertenece"
        );
    }
}
