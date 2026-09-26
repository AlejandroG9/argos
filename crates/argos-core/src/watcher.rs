use crate::monitor::{Monitor, MonitorConfig, Snapshot};
use crate::scope::Scope;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoSondeo {
    Vivo,
    Detenido,
}

struct Compartido {
    ultimo: Mutex<Option<Snapshot>>,
    scope: Mutex<Scope>,
    corriendo: AtomicBool,
    vivo: AtomicBool,
}

/// Sondea en un hilo aparte y publica el último resultado. La UI lee sin
/// esperar nunca: un ciclo lento retrasa la actualización, no la ventana.
pub struct Watcher {
    compartido: Arc<Compartido>,
    hilo: Option<JoinHandle<()>>,
}

impl Watcher {
    pub fn start(config: MonitorConfig, intervalo: Duration) -> Watcher {
        let compartido = Arc::new(Compartido {
            ultimo: Mutex::new(None),
            scope: Mutex::new(config.scope.clone()),
            corriendo: AtomicBool::new(true),
            vivo: AtomicBool::new(true),
        });

        let c = compartido.clone();
        let hilo = std::thread::spawn(move || {
            let monitor = Monitor::new(config);

            while c.corriendo.load(Ordering::Relaxed) {
                let scope = c.scope.lock().map(|s| s.clone()).unwrap_or(Scope::All);
                let snapshot = monitor.poll_con_alcance(&scope);

                if let Ok(mut ultimo) = c.ultimo.lock() {
                    *ultimo = Some(snapshot);
                }

                std::thread::sleep(intervalo);
            }

            c.vivo.store(false, Ordering::Relaxed);
        });

        Watcher {
            compartido,
            hilo: Some(hilo),
        }
    }

    /// No bloquea. Devuelve `None` hasta que hay un primer resultado.
    pub fn latest(&self) -> Option<Snapshot> {
        self.compartido.ultimo.lock().ok()?.clone()
    }

    pub fn set_scope(&self, scope: Scope) {
        if let Ok(mut s) = self.compartido.scope.lock() {
            *s = scope;
        }
    }

    pub fn scope_actual(&self) -> Scope {
        self.compartido
            .scope
            .lock()
            .map(|s| s.clone())
            .unwrap_or(Scope::All)
    }

    pub fn estado(&self) -> EstadoSondeo {
        if self.compartido.vivo.load(Ordering::Relaxed) {
            EstadoSondeo::Vivo
        } else {
            EstadoSondeo::Detenido
        }
    }

    pub fn stop(&self) {
        self.compartido.corriendo.store(false, Ordering::Relaxed);
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
        if let Some(h) = self.hilo.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn config_sin_alcance() -> MonitorConfig {
        MonitorConfig {
            scope: Scope::projects(vec![]),
            ..Default::default()
        }
    }

    fn esperar_hasta(mut cond: impl FnMut() -> bool) -> bool {
        let limite = std::time::Instant::now() + Duration::from_secs(5);
        while !cond() && std::time::Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }
        cond()
    }

    #[test]
    fn publica_un_resultado_sin_que_el_llamador_espere() {
        let w = Watcher::start(config_sin_alcance(), Duration::from_millis(20));

        // `latest()` no bloquea: al principio puede no haber nada todavía.
        let _ = w.latest();

        assert!(
            esperar_hasta(|| w.latest().is_some()),
            "el hilo debe publicar un snapshot"
        );
        w.stop();
    }

    #[test]
    fn cambiar_el_alcance_se_refleja_en_el_siguiente_ciclo() {
        let w = Watcher::start(config_sin_alcance(), Duration::from_millis(20));
        let nuevo = Scope::projects(vec![PathBuf::from("/p/nuevo")]);
        w.set_scope(nuevo.clone());

        assert!(esperar_hasta(|| w.scope_actual() == nuevo));
        w.stop();
    }

    /// Review Focus #5: si el hilo muere, la ventana no puede quedarse
    /// mostrando datos viejos en silencio para siempre.
    #[test]
    fn si_el_hilo_muere_el_estado_lo_dice() {
        let w = Watcher::start(config_sin_alcance(), Duration::from_millis(20));
        assert_eq!(w.estado(), EstadoSondeo::Vivo);

        w.stop();

        assert!(esperar_hasta(|| w.estado() == EstadoSondeo::Detenido));
    }
}
