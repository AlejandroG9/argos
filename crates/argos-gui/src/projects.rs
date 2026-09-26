use crate::app::Filter;
use crate::ventana::Ventana;
use argos_core::model::AgentState;
use argos_core::store::SessionRow;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Una fila de la pantalla de entrada: un proyecto y lo que pasa dentro.
pub struct ProjectSummary {
    pub project: Option<PathBuf>,
    /// Nombre corto para mostrar: el último segmento de la ruta.
    pub nombre: String,
    pub total: usize,
    pub esperando: usize,
    pub trabajando: usize,
    /// El estado más urgente que contiene, que decide su posición e indicador.
    pub estado: AgentState,
}

pub fn nombre_de_proyecto(project: Option<&PathBuf>) -> String {
    match project {
        Some(p) => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned()),
        None => "(sin proyecto)".to_string(),
    }
}

/// Agrupa las sesiones por proyecto para la pantalla de entrada.
///
/// Los proyectos se ordenan por urgencia y no alfabéticamente: uno con un
/// agente esperándote va arriba de uno que solo tiene trabajo en curso. El
/// nombre desempata para que la lista no baile entre refrescos.
pub fn summarize_projects(
    rows: &[SessionRow],
    filter: Filter,
    ventana: Ventana,
    now: DateTime<Utc>,
) -> Vec<ProjectSummary> {
    let mut por_proyecto: BTreeMap<Option<PathBuf>, Vec<&SessionRow>> = BTreeMap::new();

    for row in rows
        .iter()
        .filter(|r| filter.acepta(r.state) && ventana.acepta(r.last_activity, now))
    {
        por_proyecto
            .entry(row.project.clone())
            .or_default()
            .push(row);
    }

    let mut resumen: Vec<ProjectSummary> = por_proyecto
        .into_iter()
        .map(|(project, filas)| ProjectSummary {
            nombre: nombre_de_proyecto(project.as_ref()),
            total: filas.len(),
            esperando: filas
                .iter()
                .filter(|f| f.state == AgentState::Waiting)
                .count(),
            trabajando: filas
                .iter()
                .filter(|f| f.state == AgentState::Working)
                .count(),
            estado: filas
                .iter()
                .map(|f| f.state)
                .min_by_key(|s| s.urgency())
                .unwrap_or(AgentState::Unknown),
            project,
        })
        .collect();

    resumen.sort_by(|a, b| {
        a.estado
            .urgency()
            .cmp(&b.estado.urgency())
            .then_with(|| a.nombre.cmp(&b.nombre))
    });

    resumen
}

#[cfg(test)]
mod tests {
    use super::*;
    use argos_core::model::{ClientKind, Confidence};

    fn fila(proyecto: Option<&str>, state: AgentState) -> SessionRow {
        SessionRow {
            id: format!("{proyecto:?}-{state:?}-{}", rand_sufijo()),
            client: ClientKind::ClaudeCode,
            anchor_path: PathBuf::from("/x"),
            project: proyecto.map(PathBuf::from),
            branch: Some("main".into()),
            warp_focus_url: None,
            pid: None,
            started_at: None,
            last_activity: Utc::now(),
            state,
            confidence: Confidence::High,
            parent_id: None,
            depth: 0,
            metrics: None,
        }
    }

    fn rand_sufijo() -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn el_nombre_es_el_ultimo_segmento_de_la_ruta() {
        assert_eq!(
            nombre_de_proyecto(Some(&PathBuf::from("/Users/alex/Proyectos/Orion"))),
            "Orion"
        );
    }

    #[test]
    fn una_sesion_fuera_de_todo_repo_cae_en_su_propio_grupo() {
        let resumen = summarize_projects(
            &[fila(None, AgentState::Working)],
            Filter::All,
            Ventana::Todo,
            Utc::now(),
        );
        assert_eq!(resumen.len(), 1);
        assert_eq!(resumen[0].nombre, "(sin proyecto)");
        assert_eq!(resumen[0].project, None);
    }

    #[test]
    fn los_proyectos_se_ordenan_por_urgencia_no_por_nombre() {
        let rows = vec![
            fila(Some("/p/aaa"), AgentState::Finished),
            fila(Some("/p/zzz"), AgentState::Waiting),
            fila(Some("/p/mmm"), AgentState::Working),
        ];

        let nombres: Vec<String> =
            summarize_projects(&rows, Filter::All, Ventana::Todo, Utc::now())
                .into_iter()
                .map(|p| p.nombre)
                .collect();

        assert_eq!(nombres, vec!["zzz", "mmm", "aaa"]);
    }

    #[test]
    fn cuenta_cuantos_esperan_y_cuantos_trabajan_en_cada_proyecto() {
        let rows = vec![
            fila(Some("/p/orion"), AgentState::Waiting),
            fila(Some("/p/orion"), AgentState::Waiting),
            fila(Some("/p/orion"), AgentState::Working),
            fila(Some("/p/orion"), AgentState::Finished),
        ];

        let r = summarize_projects(&rows, Filter::All, Ventana::Todo, Utc::now());
        assert_eq!(r[0].total, 4);
        assert_eq!(r[0].esperando, 2);
        assert_eq!(r[0].trabajando, 1);
        assert_eq!(r[0].estado, AgentState::Waiting, "el más urgente manda");
    }

    /// Con el filtro puesto, un proyecto sin nada que mostrar desaparece de
    /// la lista en vez de aparecer vacío.
    #[test]
    fn el_filtro_elimina_los_proyectos_sin_coincidencias() {
        let rows = vec![
            fila(Some("/p/activo"), AgentState::Waiting),
            fila(Some("/p/dormido"), AgentState::Finished),
        ];

        let r = summarize_projects(&rows, Filter::NeedsAttention, Ventana::Todo, Utc::now());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].nombre, "activo");
    }

    #[test]
    fn los_conteos_reflejan_el_filtro_aplicado() {
        let rows = vec![
            fila(Some("/p/orion"), AgentState::Waiting),
            fila(Some("/p/orion"), AgentState::Finished),
            fila(Some("/p/orion"), AgentState::Finished),
        ];

        let r = summarize_projects(&rows, Filter::Active, Ventana::Todo, Utc::now());
        assert_eq!(r[0].total, 1, "las terminadas no cuentan con este filtro");
    }
}
