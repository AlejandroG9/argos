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

/// El mensaje del usuario inmediatamente anterior a la aparición del sha en
/// la conversación: el *por qué* de ese commit, que git no guarda en ninguna
/// parte.
///
/// Se recorre línea por línea recordando el último prompt humano. Los
/// resultados de herramienta también viajan con `role: "user"`, así que hay
/// que distinguirlos: solo cuenta un bloque de texto escrito por la persona.
pub fn prompt_previo(texto: &str, sha: &str) -> Option<String> {
    let mut ultimo: Option<String> = None;

    for linea in texto.lines() {
        if linea.contains(sha) {
            return ultimo;
        }

        let Ok(entrada) = serde_json::from_str::<serde_json::Value>(linea) else {
            continue;
        };
        let Some(mensaje) = entrada.get("message") else {
            continue;
        };
        if mensaje.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }

        if let Some(t) = texto_humano(mensaje.get("content")) {
            ultimo = Some(t);
        }
    }

    None
}

fn texto_humano(contenido: Option<&serde_json::Value>) -> Option<String> {
    let contenido = contenido?;

    // Claude Code guarda el mensaje como cadena suelta o como bloques.
    if let Some(t) = contenido.as_str() {
        let t = t.trim();
        return (!t.is_empty()).then(|| t.to_string());
    }

    let bloques = contenido.as_array()?;

    // Si hay un tool_result, la entrada es salida de herramienta y no tuya.
    if bloques
        .iter()
        .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
    {
        return None;
    }

    let texto: String = bloques
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join(" ");

    let texto = texto.trim();
    (!texto.is_empty()).then(|| texto.to_string())
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

    /// El caso que da valor a todo esto: recuperar la petición que llevó a
    /// ese commit, que no existe en git.
    #[test]
    fn recupera_el_mensaje_del_usuario_anterior_al_commit() {
        let log = [
            r#"{"message":{"role":"user","content":[{"type":"text","text":"arregla el filtro temporal"}]}}"#,
            r#"{"message":{"role":"assistant","content":[{"type":"text","text":"voy"}]}}"#,
            r#"{"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"x"}]}}"#,
            r#"{"tool":"Bash","stdout":"[main 042b8776] fix: filtro"}"#,
        ]
        .join("\n");

        assert_eq!(
            prompt_previo(&log, "042b8776").as_deref(),
            Some("arregla el filtro temporal")
        );
    }

    /// Los resultados de herramienta llegan con role "user" y no son tuyos:
    /// tomarlos por el prompt mostraría la salida de un comando como si
    /// fuese lo que pediste.
    #[test]
    fn un_resultado_de_herramienta_no_se_confunde_con_una_peticion() {
        let log = [
            r#"{"message":{"role":"user","content":[{"type":"text","text":"la petición real"}]}}"#,
            r#"{"message":{"role":"user","content":[{"type":"tool_result","content":"salida de un comando"}]}}"#,
            r#"{"stdout":"[main abc12345] algo"}"#,
        ]
        .join("\n");

        assert_eq!(
            prompt_previo(&log, "abc12345").as_deref(),
            Some("la petición real")
        );
    }

    #[test]
    fn se_queda_con_la_peticion_mas_cercana_al_commit() {
        let log = [
            r#"{"message":{"role":"user","content":[{"type":"text","text":"la vieja"}]}}"#,
            r#"{"message":{"role":"user","content":[{"type":"text","text":"la de justo antes"}]}}"#,
            r#"{"stdout":"[main abc12345] algo"}"#,
        ]
        .join("\n");

        assert_eq!(
            prompt_previo(&log, "abc12345").as_deref(),
            Some("la de justo antes")
        );
    }

    #[test]
    fn un_mensaje_guardado_como_cadena_suelta_tambien_vale() {
        let log = [
            r#"{"message":{"role":"user","content":"petición en texto plano"}}"#,
            r#"{"stdout":"[main abc12345] algo"}"#,
        ]
        .join("\n");

        assert_eq!(
            prompt_previo(&log, "abc12345").as_deref(),
            Some("petición en texto plano")
        );
    }

    #[test]
    fn un_sha_que_no_aparece_no_tiene_peticion() {
        let log = r#"{"message":{"role":"user","content":[{"type":"text","text":"hola"}]}}"#;
        assert_eq!(prompt_previo(log, "noexiste"), None);
    }

    #[test]
    fn un_commit_sin_peticion_previa_devuelve_nada() {
        assert_eq!(
            prompt_previo(r#"{"stdout":"[main abc12345] x"}"#, "abc12345"),
            None
        );
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
