use crate::observation::SessionObservation;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Identidad barata de un archivo. El tamaño acompaña al mtime porque dos
/// escrituras dentro del mismo segundo pueden dejar la fecha igual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Huella {
    pub mtime: SystemTime,
    pub tamano: u64,
}

impl Huella {
    pub fn de(path: &Path) -> Option<Huella> {
        let md = std::fs::metadata(path).ok()?;
        Some(Huella {
            mtime: md.modified().ok()?,
            tamano: md.len(),
        })
    }
}

/// Evita reparsear archivos que no cambiaron entre ciclos de sondeo.
#[derive(Default)]
pub struct ParseCache {
    entradas: Mutex<HashMap<PathBuf, (Huella, Option<SessionObservation>)>>,
}

impl ParseCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_parse<F>(&self, path: &Path, parsear: F) -> Option<SessionObservation>
    where
        F: FnOnce(&str) -> Option<SessionObservation>,
    {
        let huella = Huella::de(path)?;

        if let Ok(entradas) = self.entradas.lock()
            && let Some((previa, resultado)) = entradas.get(path)
            && previa == &huella
        {
            return resultado.clone();
        }

        let contenido = std::fs::read_to_string(path).ok()?;
        let resultado = parsear(&contenido);

        if let Ok(mut entradas) = self.entradas.lock() {
            entradas.insert(path.to_path_buf(), (huella, resultado.clone()));
        }

        resultado
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ruta_temporal(nombre: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("argos-cache-{}-{nombre}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn un_archivo_que_no_cambio_no_se_vuelve_a_parsear() {
        let ruta = ruta_temporal("estable");
        std::fs::write(&ruta, "contenido").expect("escribir");

        let cache = ParseCache::new();
        let veces = AtomicUsize::new(0);

        let parsear = |_: &str| {
            veces.fetch_add(1, Ordering::Relaxed);
            None::<crate::observation::SessionObservation>
        };

        cache.get_or_parse(&ruta, parsear);
        cache.get_or_parse(&ruta, parsear);

        assert_eq!(veces.load(Ordering::Relaxed), 1, "solo la primera vez");
        let _ = std::fs::remove_file(&ruta);
    }

    /// Review Focus #3: dos escrituras dentro del mismo segundo pueden dejar
    /// el mtime igual. Sin el tamaño en la huella, el caché serviría datos
    /// viejos de una sesión que está activa justo ahora.
    #[test]
    fn un_cambio_de_tamano_invalida_aunque_el_mtime_no_se_mueva() {
        let ruta = ruta_temporal("mismo-segundo");
        std::fs::write(&ruta, "corto").expect("escribir");
        let antes = Huella::de(&ruta).expect("huella");

        std::fs::write(&ruta, "mucho mas largo que antes").expect("reescribir");
        let mut despues = Huella::de(&ruta).expect("huella");
        despues.mtime = antes.mtime; // simula el mismo segundo

        assert_ne!(antes, despues, "el tamaño tiene que distinguirlas");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn un_archivo_que_desaparece_no_entra_en_panico() {
        assert!(Huella::de(Path::new("/no/existe/jamas")).is_none());
    }
}
