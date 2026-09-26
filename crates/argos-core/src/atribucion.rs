use std::collections::{HashMap, HashSet};

/// Qué conversaciones **mencionan** cada commit.
///
/// Mencionar no es lo mismo que haber creado: una sesión que corrió `git log`
/// menciona commits que no hizo. Se llama así a propósito, para no prometer
/// una autoría que este método no puede demostrar.
#[derive(Default, Debug, Clone)]
pub struct Indice {
    por_sha: HashMap<String, Vec<String>>,
}

impl Indice {
    pub fn registrar(&mut self, session_id: &str, shas: HashSet<String>) {
        for sha in shas {
            let sesiones = self.por_sha.entry(sha).or_default();
            if !sesiones.iter().any(|s| s == session_id) {
                sesiones.push(session_id.to_string());
            }
        }
    }

    pub fn sesiones_de(&self, sha: &str) -> Vec<String> {
        self.por_sha.get(sha).cloned().unwrap_or_default()
    }

    pub fn commits_conocidos(&self) -> usize {
        self.por_sha.len()
    }
}

/// Extrae los tokens hexadecimales de longitud exacta que aparecen en el
/// texto. Una pasada por carácter: escanear los 671 MB de un proyecto grande
/// tarda ~1.4 s, y con el caché por fecha solo se repite lo que cambió.
pub fn shas_mencionados(texto: &str, longitud: usize) -> HashSet<String> {
    let mut encontrados = HashSet::new();
    let bytes = texto.as_bytes();
    let mut inicio = 0usize;

    for i in 0..=bytes.len() {
        let es_hex = i < bytes.len() && bytes[i].is_ascii_hexdigit();

        if es_hex {
            continue;
        }

        // Un token vale solo si mide exactamente lo que mide el sha: uno más
        // largo es otra cosa (un uuid, un hash de archivo) que empieza igual.
        if i - inicio == longitud
            && let Ok(token) = std::str::from_utf8(&bytes[inicio..i])
            && token.bytes().any(|b| b.is_ascii_digit())
        {
            encontrados.insert(token.to_ascii_lowercase());
        }

        inicio = i + 1;
    }

    encontrados
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encuentra_los_shas_que_aparecen_en_el_texto() {
        let texto = "corriendo git commit\nsalida: [main 042b8776] feat: algo\n";
        let shas = shas_mencionados(texto, 8);

        assert!(shas.contains("042b8776"));
    }

    #[test]
    fn ignora_palabras_que_no_son_hexadecimales() {
        let shas = shas_mencionados("zzzzzzzz notahex12 GGGGGGGG", 8);
        assert!(shas.is_empty());
    }

    /// Un token más largo o más corto que el sha no es ese sha. Sin esto,
    /// cualquier uuid o hash de archivo produciría coincidencias falsas.
    #[test]
    fn respeta_la_longitud_exacta_del_sha() {
        let shas = shas_mencionados("042b877 042b87765 042b8776", 8);

        assert!(shas.contains("042b8776"), "el de 8 sí");
        assert!(!shas.contains("042b877"), "el de 7 no");
        assert!(!shas.contains("042b87765"), "el de 9 tampoco");
    }

    #[test]
    fn un_sha_dentro_de_json_se_reconoce_igual() {
        let texto = r#"{"stdout":"[main 042b8776] feat: algo\n 3 files changed"}"#;
        assert!(shas_mencionados(texto, 8).contains("042b8776"));
    }

    #[test]
    fn invierte_el_indice_de_sesiones_a_commits() {
        let mut indice = Indice::default();
        indice.registrar("sesion-a", ["042b8776".to_string()].into());
        indice.registrar(
            "sesion-b",
            ["042b8776".to_string(), "aaaa1111".to_string()].into(),
        );

        let mut participantes = indice.sesiones_de("042b8776");
        participantes.sort();
        assert_eq!(participantes, vec!["sesion-a", "sesion-b"]);

        assert_eq!(indice.sesiones_de("aaaa1111"), vec!["sesion-b"]);
        assert!(indice.sesiones_de("desconocido").is_empty());
    }

    #[test]
    fn registrar_dos_veces_la_misma_sesion_no_la_duplica() {
        let mut indice = Indice::default();
        indice.registrar("s", ["042b8776".to_string()].into());
        indice.registrar("s", ["042b8776".to_string()].into());

        assert_eq!(indice.sesiones_de("042b8776").len(), 1);
    }
}
