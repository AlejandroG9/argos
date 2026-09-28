use crate::discovery::{Worktree, worktree_containing};
use crate::model::Confidence;
use crate::observation::{ProcessObservation, SessionObservation};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

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

    // Solo sesiones raíz: los subagentes corren dentro del proceso de su
    // padre y no compiten por uno propio.
    let raices: Vec<&SessionObservation> =
        sessions.iter().filter(|s| s.parent_id.is_none()).collect();

    // Un proceso es **una** sesión. Se resuelve como una asignación global y
    // no grupo por grupo: al aceptar directorios ancestro, un `claude` en
    // `~/Proyectos` encaja con sesiones de todos sus subdirectorios, y
    // repartirlo entre varias haría que el salto a la terminal llevara a la
    // conversación equivocada.
    let mut parejas: Vec<(usize, usize, bool, DateTime<Utc>, i64)> = Vec::new();

    for (is, session) in raices.iter().enumerate() {
        for (ip, proc) in procs.iter().enumerate() {
            if proc.client != session.client {
                continue;
            }
            // El cwd del proceso es donde el agente arrancó; el de la sesión,
            // donde trabaja. Vale el mismo directorio o cualquier ancestro.
            let Some(cwd) = proc.cwd.as_deref() else {
                continue;
            };
            if !session.anchor_path.starts_with(cwd) {
                continue;
            }
            // …pero el ancestro tiene que seguir siendo el mismo repositorio.
            //
            // Sin esto, un agente lanzado en la carpeta que contiene todos los
            // proyectos es ancestro de todos y elegible para cada sesión de
            // cada uno: Orion mostraba dos agentes trabajando teniendo uno.
            //
            // Si no se conoce el repositorio de la sesión no se inventa nada y
            // manda la regla de arriba: quedarse sin correlacionar por un
            // descubrimiento incompleto sería peor que correlacionar de más.
            if let Some(repo) = worktree_containing(worktrees, &session.anchor_path)
                && !cwd.starts_with(&repo.repo_root)
            {
                continue;
            }
            // Un proceso que arrancó después de la última actividad no pudo
            // haberla producido.
            if proc.started_at > session.last_activity {
                continue;
            }

            let exacto = cwd == session.anchor_path;
            let referencia = session.first_seen.unwrap_or(session.last_activity);
            parejas.push((
                is,
                ip,
                exacto,
                session.last_activity,
                distancia(proc.started_at, referencia),
            ));
        }
    }

    // Primero las coincidencias exactas de ruta. Entre las demás manda la
    // actividad más reciente: el proceso está escribiendo en la sesión que
    // acaba de moverse, no en una que lleva días quieta. La cercanía del
    // arranque y los índices solo desempatan, para que el reparto no dependa
    // del orden de recorrido de un HashMap.
    parejas.sort_by_key(|(is, ip, exacto, ultima, dist)| {
        (!*exacto, std::cmp::Reverse(*ultima), *dist, *is, *ip)
    });

    // Una pareja disputada es una conjetura aunque la ruta coincida exacto:
    // con dos procesos y dos sesiones en el mismo directorio, cuál va con
    // cuál se decide por cercanía temporal, que es heurística.
    let mut candidatos_por_sesion = vec![0usize; raices.len()];
    let mut candidatos_por_proceso = vec![0usize; procs.len()];
    for (is, ip, _, _, _) in &parejas {
        candidatos_por_sesion[*is] += 1;
        candidatos_por_proceso[*ip] += 1;
    }

    let mut sesiones_tomadas = vec![false; raices.len()];
    let mut procesos_tomados = vec![false; procs.len()];
    let mut asignado: Vec<Option<usize>> = vec![None; raices.len()];

    for (is, ip, _, _, _) in parejas {
        if sesiones_tomadas[is] || procesos_tomados[ip] {
            continue;
        }
        sesiones_tomadas[is] = true;
        procesos_tomados[ip] = true;
        asignado[is] = Some(ip);
    }

    for (is, session) in raices.iter().enumerate() {
        let elegido = asignado[is].map(|ip| procs[ip].clone());

        if let Some(p) = &elegido {
            proceso_por_sesion.insert(session.id.clone(), p.clone());
        }

        let worktree = worktree_containing(worktrees, &session.anchor_path).cloned();

        // Ambiguo cuando el proceso no estaba exactamente en el directorio de
        // la sesión: el emparejamiento es entonces una inferencia, no un dato.
        let exacto = elegido
            .as_ref()
            .and_then(|p| p.cwd.as_deref())
            .is_some_and(|cwd| cwd == session.anchor_path);

        let disputado = candidatos_por_sesion[is] > 1
            || asignado[is].is_some_and(|ip| candidatos_por_proceso[ip] > 1);

        let confianza = match (&elegido, &worktree) {
            (_, None) => Confidence::Low,
            (None, _) => Confidence::Low,
            (Some(_), Some(_)) if exacto && !disputado => Confidence::High,
            (Some(_), Some(_)) => Confidence::Medium,
        };

        resultado.push(Correlated {
            session: (*session).clone(),
            process: elegido,
            worktree,
            confidence: confianza,
        });
    }

    // Segunda pasada: los subagentes heredan el proceso de su padre.
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
    use crate::model::ClientKind;
    use crate::model::Confidence;
    use crate::observation::ActivitySemantics;
    use chrono::{Duration, TimeZone, Utc};
    use std::path::PathBuf;

    fn t(offset_secs: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap() + Duration::seconds(offset_secs)
    }

    /// Un agente lanzado en la carpeta que contiene *todos* los proyectos no
    /// está trabajando en ninguno en concreto, y desde luego no en todos a la
    /// vez. Sin esta regla, un `claude` en ~/Proyectos era elegible para cada
    /// sesión de cada proyecto de debajo y reclamaba una: Orion mostraba dos
    /// agentes trabajando teniendo uno solo.
    #[test]
    fn un_proceso_del_directorio_padre_no_reclama_sesiones_de_los_proyectos() {
        let dentro = ProcessObservation {
            cwd: Some(PathBuf::from("/proyectos/orion")),
            ..proceso(63062, "/proyectos/orion", 0)
        };
        let fuera = ProcessObservation {
            cwd: Some(PathBuf::from("/proyectos")),
            ..proceso(24569, "/proyectos", 0)
        };

        let sesiones = vec![
            sesion("viva", "/proyectos/orion", 10, 20),
            sesion("vieja", "/proyectos/orion", 5, 15),
        ];
        let worktrees = vec![Worktree {
            path: PathBuf::from("/proyectos/orion"),
            branch: Some("main".into()),
            repo_root: PathBuf::from("/proyectos/orion"),
        }];

        let r = correlate(&[dentro, fuera], &sesiones, &worktrees);

        let con_proceso = r.iter().filter(|c| c.process.is_some()).count();
        assert_eq!(
            con_proceso, 1,
            "solo el proceso que corre dentro del repo puede reclamar una sesión"
        );
        assert_eq!(
            r.iter()
                .find(|c| c.process.is_some())
                .and_then(|c| c.process.as_ref())
                .map(|p| p.pid),
            Some(63062)
        );
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

    /// Un proceso es **una** sesión. Al aceptar ancestros, un `claude` en
    /// `~/Proyectos` encaja con sesiones de todos sus subdirectorios; si cada
    /// grupo lleva su propia lista de usados, el mismo proceso se reparte
    /// entre proyectos y el salto a la terminal lleva a la conversación
    /// equivocada.
    #[test]
    fn un_proceso_no_se_reparte_entre_sesiones_de_proyectos_distintos() {
        let procs = vec![proceso(100, "/repo", 0)];
        let sesiones = vec![
            sesion("a", "/repo/uno", 10, 20),
            sesion("b", "/repo/dos", 10, 20),
        ];
        let worktrees = vec![worktree("/repo/uno", "main"), worktree("/repo/dos", "main")];

        let resultado = correlate(&procs, &sesiones, &worktrees);

        let con_proceso = resultado.iter().filter(|c| c.process.is_some()).count();
        assert_eq!(con_proceso, 1, "el proceso solo corre una de las dos");
    }

    /// Con dos candidatas, se queda con la que de verdad está en ese
    /// directorio antes que con una descendiente.
    #[test]
    fn el_proceso_prefiere_la_sesion_de_su_propio_directorio() {
        let procs = vec![proceso(100, "/repo", 0)];
        let sesiones = vec![
            sesion("descendiente", "/repo/sub", 10, 20),
            sesion("exacta", "/repo", 10, 20),
        ];
        let worktrees = vec![worktree("/repo", "main"), worktree("/repo/sub", "otra")];

        let resultado = correlate(&procs, &sesiones, &worktrees);
        let exacta = resultado.iter().find(|c| c.session.id == "exacta").unwrap();

        assert_eq!(exacta.process.as_ref().map(|p| p.pid), Some(100));
    }

    /// El reparto no puede depender del orden de recorrido de un HashMap:
    /// dos ejecuciones con los mismos datos deben dar el mismo resultado.
    #[test]
    fn el_emparejamiento_es_determinista() {
        let procs = vec![proceso(100, "/repo", 0), proceso(200, "/repo", 1)];
        let sesiones = vec![
            sesion("a", "/repo/uno", 10, 20),
            sesion("b", "/repo/dos", 10, 20),
            sesion("c", "/repo/tres", 10, 20),
        ];
        let worktrees = vec![
            worktree("/repo/uno", "x"),
            worktree("/repo/dos", "y"),
            worktree("/repo/tres", "z"),
        ];

        let primera: Vec<_> = correlate(&procs, &sesiones, &worktrees)
            .iter()
            .map(|c| (c.session.id.clone(), c.process.as_ref().map(|p| p.pid)))
            .collect();

        for _ in 0..12 {
            let otra: Vec<_> = correlate(&procs, &sesiones, &worktrees)
                .iter()
                .map(|c| (c.session.id.clone(), c.process.as_ref().map(|p| p.pid)))
                .collect();
            assert_eq!(primera, otra, "mismo dato, mismo reparto");
        }
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
