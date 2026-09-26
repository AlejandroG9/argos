use argos_core::monitor::{Monitor, MonitorConfig};

fn main() {
    let monitor = Monitor::new(MonitorConfig::default());

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
            "[{:?}] {:<14} {:<28} {} (confianza {:?}){}",
            row.state,
            row.client.label(),
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

    if !snapshot.degraded.is_empty() {
        println!("\nPlataformas degradadas:");
        for (client, motivo) in &snapshot.degraded {
            println!("  {:?}: {motivo}", client);
        }
    }
}
