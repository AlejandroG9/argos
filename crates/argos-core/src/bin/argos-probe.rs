use argos_core::monitor::{Monitor, MonitorConfig};
use argos_core::scope::Scope;

fn main() {
    // `--scope <ruta>` reproduce lo que ve la app con ese proyecto abierto.
    // Sin él, la sonda mira la máquina entera, que es útil para comparar pero
    // **no** es lo que la interfaz muestra: confundir las dos cosas manda una
    // investigación por el camino equivocado.
    let args: Vec<String> = std::env::args().collect();
    let scope = args
        .iter()
        .position(|a| a == "--scope")
        .and_then(|i| args.get(i + 1))
        .map(|p| Scope::projects(vec![std::path::PathBuf::from(p)]))
        .unwrap_or_else(Scope::all);

    println!("Alcance: {scope:?}\n");

    let monitor = Monitor::new(MonitorConfig {
        scope,
        ..MonitorConfig::default()
    });

    let reindexar = std::env::args().any(|a| a == "--reindex");
    let snapshot = if reindexar {
        match monitor.reindex() {
            Ok(s) => {
                println!("Base reconstruida desde los logs.\n");
                s
            }
            Err(e) => {
                eprintln!("no se pudo reindexar: {e}");
                return;
            }
        }
    } else {
        monitor.poll()
    };

    println!("Snapshot de {}", snapshot.taken_at.to_rfc3339());
    println!("{} sesiones\n", snapshot.rows.len());

    for row in &snapshot.rows {
        println!(
            "[{:?}] {:<14} pid={:<8} {:<20} {} (confianza {:?}){}",
            row.state,
            row.client.label(),
            row.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            row.branch.as_deref().unwrap_or("(sin rama)"),
            row.anchor_path.display(),
            row.confidence,
            if row.parent_id.is_some() {
                "  ↳ subagente"
            } else {
                ""
            },
        );
    }

    if let Some(err) = &snapshot.persist_error {
        println!("\nNo se pudo guardar el histórico: {err}");
    }

    if !snapshot.degraded.is_empty() {
        println!("\nPlataformas degradadas:");
        for (client, motivo) in &snapshot.degraded {
            println!("  {:?}: {motivo}", client);
        }
    }
}
