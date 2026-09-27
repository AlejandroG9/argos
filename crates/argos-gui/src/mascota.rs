use argos_core::model::AgentState;

/// Una fila del atlas: una animación completa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tira {
    pub fila: usize,
    pub fotogramas: usize,
}

/// El atlas de Benny fue dibujado con los mismos estados que Argos infiere:
/// la fila `waiting` dice literalmente "waiting for approval, help, or user
/// input" y `running`, "active task work or processing". No hay que forzar
/// nada — cada estado tiene su animación propia.
pub fn tira_de(estado: AgentState) -> Tira {
    match estado {
        AgentState::Working => Tira {
            fila: 7,
            fotogramas: 6,
        },
        AgentState::Waiting => Tira {
            fila: 6,
            fotogramas: 6,
        },
        AgentState::Finished => Tira {
            fila: 8,
            fotogramas: 6,
        },
        // "blocked, failed, or cancelled": lo más cercano a no saber qué pasa.
        AgentState::Unknown => Tira {
            fila: 5,
            fotogramas: 8,
        },
    }
}

/// Fotogramas por segundo de la mascota. Lento a propósito: es un indicador
/// de estado en una app de fondo, no un videojuego.
const FPS: f32 = 7.0;

pub fn fotograma_en(tira: Tira, t: f32) -> usize {
    ((t * FPS) as usize) % tira.fotogramas.max(1)
}

/// Rectángulo normalizado (0..1) de un fotograma dentro del atlas, que es lo
/// que egui necesita para recortar la textura.
pub fn uv_de(tira: Tira, fotograma: usize, columnas: usize, filas: usize) -> egui::Rect {
    let ancho = 1.0 / columnas as f32;
    let alto = 1.0 / filas as f32;
    let x = (fotograma % columnas) as f32 * ancho;
    let y = tira.fila as f32 * alto;

    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(ancho, alto))
}

#[derive(Default)]
pub struct Mascota {
    textura: Option<Option<egui::TextureHandle>>,
    pub columnas: usize,
    pub filas: usize,
}

impl Mascota {
    pub fn carpeta() -> std::path::PathBuf {
        std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".argos/mascota")
    }

    /// Carga perezosa y recordada, incluido el fallo: sin mascota instalada
    /// el nodo enseña la inicial y no se reintenta en cada cuadro.
    ///
    /// Devuelve el id de textura, no el manejador, para que quien dibuja
    /// pueda seguir leyendo la rejilla sin chocar con el préstamo.
    pub fn textura(&mut self, ctx: &egui::Context) -> Option<(egui::TextureId, usize, usize)> {
        if self.textura.is_none() {
            let (tex, cols, fils) = cargar(ctx);
            self.columnas = cols;
            self.filas = fils;
            self.textura = Some(tex);
        }

        let id = self.textura.as_ref()?.as_ref()?.id();
        Some((id, self.columnas, self.filas))
    }
}

fn cargar(ctx: &egui::Context) -> (Option<egui::TextureHandle>, usize, usize) {
    let carpeta = Mascota::carpeta();

    // La rejilla la declara el propio paquete; suponerla produciría recortes
    // desalineados si el atlas cambia de versión.
    let (columnas, filas) = std::fs::read_to_string(carpeta.join("atlas.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| {
            let a = v.get("atlas")?;
            Some((
                a.get("columns")?.as_u64()? as usize,
                a.get("rows")?.as_u64()? as usize,
            ))
        })
        .unwrap_or((8, 11));

    let Ok(bytes) = std::fs::read(carpeta.join("spritesheet.webp")) else {
        return (None, columnas, filas);
    };
    let Ok(imagen) = image::load_from_memory(&bytes) else {
        return (None, columnas, filas);
    };

    let rgba = imagen.to_rgba8();
    let tamano = [rgba.width() as usize, rgba.height() as usize];

    (
        Some(ctx.load_texture(
            "mascota",
            egui::ColorImage::from_rgba_unmultiplied(tamano, rgba.as_raw()),
            egui::TextureOptions::LINEAR,
        )),
        columnas,
        filas,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El atlas trae una animación por estado; usar la equivocada diría algo
    /// falso sobre lo que el agente está haciendo.
    #[test]
    fn cada_estado_usa_su_propia_animacion() {
        let filas: Vec<usize> = [
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Finished,
            AgentState::Unknown,
        ]
        .into_iter()
        .map(|e| tira_de(e).fila)
        .collect();

        assert_eq!(filas, vec![7, 6, 8, 5]);
        assert_eq!(
            filas.iter().collect::<std::collections::HashSet<_>>().len(),
            4,
            "ninguna se repite"
        );
    }

    #[test]
    fn la_animacion_recorre_sus_fotogramas_y_vuelve_a_empezar() {
        let tira = Tira {
            fila: 7,
            fotogramas: 6,
        };

        assert_eq!(fotograma_en(tira, 0.0), 0);
        assert_eq!(fotograma_en(tira, 1.0 / FPS), 1);
        // Al completar la vuelta regresa al primero.
        assert_eq!(fotograma_en(tira, 6.0 / FPS), 0);
    }

    #[test]
    fn una_tira_sin_fotogramas_no_divide_entre_cero() {
        let tira = Tira {
            fila: 0,
            fotogramas: 0,
        };
        assert_eq!(fotograma_en(tira, 3.0), 0);
    }

    #[test]
    fn el_recorte_apunta_a_la_celda_correcta_del_atlas() {
        let tira = Tira {
            fila: 7,
            fotogramas: 6,
        };
        let uv = uv_de(tira, 2, 8, 11);

        assert!((uv.min.x - 2.0 / 8.0).abs() < 1e-6, "tercera columna");
        assert!((uv.min.y - 7.0 / 11.0).abs() < 1e-6, "octava fila");
        assert!((uv.width() - 1.0 / 8.0).abs() < 1e-6);
        assert!((uv.height() - 1.0 / 11.0).abs() < 1e-6);
    }

    /// Una tira más larga que el ancho del atlas se enrollaría a la fila
    /// siguiente; ninguna de las de Benny lo hace, y el test lo fija.
    #[test]
    fn ninguna_animacion_excede_el_ancho_del_atlas() {
        for estado in [
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Finished,
            AgentState::Unknown,
        ] {
            assert!(tira_de(estado).fotogramas <= 8, "caben en 8 columnas");
        }
    }
}
