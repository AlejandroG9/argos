use crate::discovery::{Worktree, worktree_containing};
use crate::model::{ClientKind, Confidence};
use crate::observation::{ProcessObservation, SessionObservation};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Correlated {
    pub session: SessionObservation,
    pub process: Option<ProcessObservation>,
    pub worktree: Option<Worktree>,
    pub confidence: Confidence,
}

pub fn correlate(
    procs: &[ProcessObservation],
    sessions: &[SessionObservation],
    worktrees: &[Worktree],
) -> Vec<Correlated> {
    let mut resultado: Vec<Correlated> = Vec::new();
    let mut proceso_por_sesion: HashMap<String, ProcessObservation> = HashMap::new();

    // Primera pasada: solo sesiones raíz. Los subagentes corren dentro del
    // proceso de su padre, así que no compiten por uno propio.
    // Se agrupan por (cliente, ruta ancla) porque la ambigüedad es local al
    // directorio: varias sesiones del mismo CLI en el mismo worktree.
    let mut grupos: HashMap<(ClientKind, PathBuf), Vec<&SessionObservation>> = HashMap::new();
    for session in sessions.iter().filter(|s| s.parent_id.is_none()) {
        grupos
            .entry((session.client, session.anchor_path.clone()))
            .or_default()
            .push(session);
    }

    for ((client, anchor), mut grupo) in grupos {
        // El cwd del proceso es donde el agente **arrancó**; el de la sesión,
        // donde está trabajando. Un agente que hizo `cd` a un subdirectorio
        // sigue siendo el mismo proceso, así que también valen los ancestros.
        // Exigir igualdad exacta lo dejaba sin proceso, y sin proceso el motor
        // lo declara terminado aunque esté ejecutando comandos.
        let en_grupo: Vec<&ProcessObservation> = procs
            .iter()
            .filter(|p| p.client == client)
            .filter(|p| p.cwd.as_deref().is_some_and(|cwd| anchor.starts_with(cwd)))
            .collect();

        // Con un solo proceso y una sola sesión no hay nada que adivinar.
        let ambiguo = en_grupo.len() > 1 || grupo.len() > 1;

        // Emparejar de la más antigua a la más reciente, para que el orden de
        // asignación sea determinista.
        grupo.sort_by_key(|s| s.first_seen.unwrap_or(s.last_activity));

        let mut usados: Vec<u32> = Vec::new();

        for session in grupo {
            let referencia = session.first_seen.unwrap_or(session.last_activity);

            // Un proceso que arrancó después de la última actividad de la
            // sesión no puede haberla producido.
            // Un candidato exacto gana a uno que solo es ancestro: es el que
            // de verdad está en ese directorio. Entre iguales, el más cercano
            // en el tiempo.
            let elegido = en_grupo
                .iter()
                .filter(|p| !usados.contains(&p.pid))
                .filter(|p| p.started_at <= session.last_activity)
                .min_by_key(|p| {
                    let exacto = p.cwd.as_deref() != Some(anchor.as_path());
                    (exacto, distancia(p.started_at, referencia))
                })
                .map(|p| (*p).clone());

            if let Some(p) = &elegido {
                usados.push(p.pid);
                proceso_por_sesion.insert(session.id.clone(), p.clone());
            }

            let worktree = worktree_containing(worktrees, &session.anchor_path).cloned();

            let confianza = match (&elegido, &worktree) {
                (_, None) => Confidence::Low,
                (None, _) => Confidence::Low,
                (Some(_), Some(_)) if ambiguo => Confidence::Medium,
                (Some(_), Some(_)) => Confidence::High,
            };

            resultado.push(Correlated {
                session: session.clone(),
                process: elegido,
                worktree,
                confidence: confianza,
            });
        }
    }

    // Segunda pasada: subagentes heredan el proceso del padre.
    for session in sessions.iter().filter(|s| s.parent_id.is_some()) {
        let process = session
            .parent_id
            .as_ref()
            .and_then(|parent| proceso_por_sesion.get(parent))
            .cloned();

        let worktree = worktree_containing(worktrees, &session.anchor_path).cloned();
        let confidence = match (&process, &worktree) {
            (_, None) => Confidence::Low,
            (Some(_), Some(_)) => Confidence::High,
            (None, Some(_)) => Confidence::Low,
        };

        resultado.push(Correlated {
            session: session.clone(),
            process,
            worktree,
            confidence,
        });
    }

    resultado
}

fn distancia(a: DateTime<Utc>, b: DateTime<Utc>) -> i64 {
    (a - b).num_seconds().abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClientKind, Confidence};
    use crate::observation::ActivitySemantics;
    use chrono::{Duration, TimeZone, Utc};
    use std::path::PathBuf;

    fn t(offset_secs: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap() + Duration::seconds(offset_secs)
    }

    fn proceso(pid: u32, cwd: &str, arranque: i64) -> ProcessObservation {
        ProcessObservation {
            pid,
            ppid: 1,
            client: ClientKind::ClaudeCode,
            cwd: Some(PathBuf::from(cwd)),
            started_at: t(arranque),
            warp_session_uuid: Some(format!("uuid-{pid}")),
            warp_focus_url: Some(format!("warp://session/uuid-{pid}")),
        }
    }

    fn sesion(id: &str, cwd: &str, primera: i64, ultima: i64) -> SessionObservation {
        SessionObservation {
            id: id.to_string(),
            client: ClientKind::ClaudeCode,
            anchor_path: PathBuf::from(cwd),
            git_branch: None,
            first_seen: Some(t(primera)),
            last_activity: t(ultima),
            activity: ActivitySemantics::AssistantTurnEnded,
            metrics: None,
            parent_id: None,
            source_path: PathBuf::from(format!("/logs/{id}.jsonl")),
        }
    }

    fn worktree(path: &str, branch: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            branch: Some(branch.to_string()),
            repo_root: PathBuf::from("/repo"),
        }
    }

    #[test]
    fn un_proceso_y_una_sesion_se_emparejan_con_confianza_alta() {
        let procs = vec![proceso(100, "/repo", 0)];
        let sesiones = vec![sesion("s1", "/repo", 10, 20)];
        let worktrees = vec![worktree("/repo", "main")];

        let resultado = correlate(&procs, &sesiones, &worktrees);

        assert_eq!(resultado.len(), 1);
        assert_eq!(resultado[0].process.as_ref().map(|p| p.pid), Some(100));
        assert_eq!(resultado[0].confidence, Confidence::High);
        assert_eq!(
            resultado[0]
                .worktree
                .as_ref()
                .and_then(|w| w.branch.clone()),
            Some("main".to_string())
        );
    }

    #[test]
    fn varias_sesiones_en_el_mismo_directorio_se_emparejan_por_cercania_temporal() {
        let procs = vec![proceso(100, "/repo", 0), proceso(200, "/repo", 100)];
        let sesiones = vec![
            sesion("vieja", "/repo", 5, 50),
            sesion("nueva", "/repo", 105, 150),
        ];
        let worktrees = vec![worktree("/repo", "main")];

        let resultado = correlate(&procs, &sesiones, &worktrees);

        let vieja = resultado.iter().find(|c| c.session.id == "vieja").unwrap();
        let nueva = resultado.iter().find(|c| c.session.id == "nueva").unwrap();

        assert_eq!(vieja.process.as_ref().map(|p| p.pid), Some(100));
        assert_eq!(nueva.process.as_ref().map(|p| p.pid), Some(200));
        // Con ambigüedad, la confianza baja: el spec exige mostrar la duda.
        assert_eq!(vieja.confidence, Confidence::Medium);
        assert_eq!(nueva.confidence, Confidence::Medium);
    }

    #[test]
    fn una_sesion_sin_proceso_queda_sin_emparejar() {
        let sesiones = vec![sesion("s1", "/repo", 10, 20)];
        let worktrees = vec![worktree("/repo", "main")];

        let resultado = correlate(&[], &sesiones, &worktrees);

        assert_eq!(resultado.len(), 1);
        assert!(resultado[0].process.is_none());
    }

    #[test]
    fn un_proceso_que_arranco_despues_de_la_ultima_actividad_no_empareja() {
        let procs = vec![proceso(100, "/repo", 500)];
        let sesiones = vec![sesion("s1", "/repo", 10, 20)];
        let worktrees = vec![worktree("/repo", "main")];

        let resultado = correlate(&procs, &sesiones, &worktrees);

        assert!(resultado[0].process.is_none());
    }

    #[test]
    fn un_subagente_hereda_el_proceso_de_su_padre() {
        let procs = vec![proceso(100, "/repo", 0)];
        let mut hijo = sesion("agent-abc", "/repo", 15, 18);
        hijo.parent_id = Some("s1".to_string());
        let sesiones = vec![sesion("s1", "/repo", 10, 20), hijo];
        let worktrees = vec![worktree("/repo", "main")];

        let resultado = correlate(&procs, &sesiones, &worktrees);
        let subagente = resultado
            .iter()
            .find(|c| c.session.id == "agent-abc")
            .unwrap();

        assert_eq!(subagente.process.as_ref().map(|p| p.pid), Some(100));
    }

    /// Caso real: un agente arranca en `~/Proyectos` y trabaja dentro de
    /// `~/Proyectos/argos`. El cwd del proceso es donde arrancó; el de la
    /// sesión, donde trabaja. Exigir igualdad exacta deja al agente sin
    /// proceso, y sin proceso el motor concluye "terminó" — de un agente que
    /// está ejecutando comandos ahora mismo.
    #[test]
    fn un_proceso_en_un_directorio_ancestro_empareja_con_la_sesion() {
        let procs = vec![proceso(100, "/repo", 0)];
        let sesiones = vec![sesion("s1", "/repo/sub/proyecto", 10, 20)];
        let worktrees = vec![worktree("/repo/sub/proyecto", "main")];

        let resultado = correlate(&procs, &sesiones, &worktrees);

        assert_eq!(
            resultado[0].process.as_ref().map(|p| p.pid),
            Some(100),
            "el proceso del padre es el que corre esta sesión"
        );
    }

    #[test]
    fn un_proceso_en_otra_rama_del_arbol_no_empareja() {
        let procs = vec![proceso(100, "/otro/sitio", 0)];
        let sesiones = vec![sesion("s1", "/repo", 10, 20)];
        let worktrees = vec![worktree("/repo", "main")];

        assert!(
            correlate(&procs, &sesiones, &worktrees)[0]
                .process
                .is_none()
        );
    }

    /// Con un candidato exacto y otro solo ancestro, gana el exacto: es el
    /// que de verdad está en ese directorio.
    #[test]
    fn un_candidato_exacto_gana_a_uno_ancestro() {
        let procs = vec![proceso(100, "/repo", 0), proceso(200, "/repo/sub", 1)];
        let sesiones = vec![sesion("s1", "/repo/sub", 10, 20)];
        let worktrees = vec![worktree("/repo/sub", "main")];

        assert_eq!(
            correlate(&procs, &sesiones, &worktrees)[0]
                .process
                .as_ref()
                .map(|p| p.pid),
            Some(200)
        );
    }

    /// Review Focus #5: `git worktree remove` mientras la sesión sigue en disco.
    #[test]
    fn una_sesion_sin_worktree_sigue_apareciendo_sin_romper_el_resto() {
        let procs = vec![proceso(100, "/repo", 0)];
        let sesiones = vec![
            sesion("viva", "/repo", 10, 20),
            sesion("huerfana", "/repo-borrado", 10, 20),
        ];
        let worktrees = vec![worktree("/repo", "main")];

        let resultado = correlate(&procs, &sesiones, &worktrees);

        assert_eq!(resultado.len(), 2, "la huérfana no debe desaparecer");
        let huerfana = resultado
            .iter()
            .find(|c| c.session.id == "huerfana")
            .unwrap();
        assert!(huerfana.worktree.is_none());
        assert_eq!(huerfana.confidence, Confidence::Low);

        let viva = resultado.iter().find(|c| c.session.id == "viva").unwrap();
        assert!(viva.worktree.is_some(), "la sana no debe verse afectada");
    }
}
