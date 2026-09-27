use crate::error::ProbeError;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Un commit tal como lo cuenta git, sin interpretar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub mensaje: String,
    pub autor: String,
    pub fecha: DateTime<Utc>,
    pub padres: Vec<String>,
    /// Quién firma como coautor. A diferencia de las menciones, esto es
    /// evidencia directa: el trailer lo escribió quien hizo el commit.
    pub coautores: Vec<String>,
    /// Ramas y etiquetas que apuntan aquí. Sin esto no se sabe qué carril
    /// es cuál.
    pub refs: Vec<String>,
}

impl Commit {
    pub fn es_merge(&self) -> bool {
        self.padres.len() > 1
    }
}

/// Separadores de unidad y de registro. Se usan en vez de `\n` porque un
/// mensaje de commit puede contener saltos de línea y partiría el registro.
const SEP_CAMPO: char = '\u{1f}';
const SEP_REGISTRO: char = '\u{1e}';

const FORMATO: &str = "--format=%h%x1f%s%x1f%an%x1f%aI%x1f%p%x1f%D%x1f%b%x1e";

pub fn parse_log(salida: &str) -> Vec<Commit> {
    salida
        .split(SEP_REGISTRO)
        .filter_map(|registro| {
            let registro = registro.trim_start_matches(['\n', '\r']);
            if registro.trim().is_empty() {
                return None;
            }

            let campos: Vec<&str> = registro.split(SEP_CAMPO).collect();
            if campos.len() < 5 {
                return None;
            }

            let fecha = DateTime::parse_from_rfc3339(campos[3]).ok()?;

            Some(Commit {
                sha: campos[0].to_string(),
                mensaje: campos[1].to_string(),
                autor: campos[2].to_string(),
                fecha: fecha.with_timezone(&Utc),
                padres: campos[4].split_whitespace().map(str::to_string).collect(),
                refs: campos.get(5).map(|d| parse_refs(d)).unwrap_or_default(),
                coautores: campos
                    .get(6)
                    .map(|b| parse_coautores(b))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// `%D` entrega algo como `HEAD -> main, origin/main, tag: v1.0`.
fn parse_refs(decoracion: &str) -> Vec<String> {
    decoracion
        .split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(|r| r.strip_prefix("HEAD -> ").unwrap_or(r).to_string())
        .filter(|r| r != "HEAD")
        .collect()
}

/// Lee los trailers `Co-Authored-By` del cuerpo del mensaje. Git no impone
/// mayúsculas, así que la comparación las ignora.
pub fn parse_coautores(cuerpo: &str) -> Vec<String> {
    cuerpo
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (clave, valor) = l.split_once(':')?;
            if !clave.trim().eq_ignore_ascii_case("co-authored-by") {
                return None;
            }
            let valor = valor.trim();
            (!valor.is_empty()).then(|| valor.to_string())
        })
        .collect()
}

/// Cuántos commits lleva cada rama por delante y por detrás de su remoto.
/// Una rama sin remoto simplemente no aparece: no hay nada con qué comparar.
pub fn parse_branch_status(salida: &str) -> HashMap<String, (u32, u32)> {
    let mut estado = HashMap::new();

    for linea in salida.lines() {
        // `*` marca la rama actual y `+` las sacadas en un worktree.
        let linea = linea.trim_start_matches(['*', '+', ' ']);
        let mut campos = linea.split_whitespace();
        let Some(rama) = campos.next() else { continue };

        let Some(inicio) = linea.find('[') else {
            continue;
        };
        let Some(fin) = linea[inicio..].find(']') else {
            continue;
        };
        let dentro = &linea[inicio + 1..inicio + fin];

        // Sin remoto, los corchetes no aparecen; con remoto al día, aparecen
        // pero sin "ahead"/"behind".
        if !dentro.contains('/') {
            continue;
        }

        estado.insert(
            rama.to_string(),
            (numero(dentro, "ahead"), numero(dentro, "behind")),
        );
    }

    estado
}

fn numero(texto: &str, etiqueta: &str) -> u32 {
    texto
        .find(etiqueta)
        .and_then(|i| {
            texto[i + etiqueta.len()..]
                .split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0)
}

pub fn leer_historia(repo: &Path, limite: usize) -> Result<Vec<Commit>, ProbeError> {
    let salida = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "--all", "--date-order", FORMATO])
        .arg(format!("-n{limite}"))
        .output()
        .map_err(|source| ProbeError::Command {
            command: "git log".into(),
            source,
        })?;

    Ok(parse_log(&String::from_utf8_lossy(&salida.stdout)))
}

pub fn leer_estado_de_ramas(repo: &Path) -> HashMap<String, (u32, u32)> {
    let Ok(salida) = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["branch", "-vv"])
        .output()
    else {
        return HashMap::new();
    };

    parse_branch_status(&String::from_utf8_lossy(&salida.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una salida como la que produce el formato de `leer_historia`: campos
    /// separados por 0x1f y registros por 0x1e.
    fn salida(registros: &[&str]) -> String {
        registros
            .iter()
            .map(|r| format!("{r}\u{1e}"))
            .collect::<String>()
    }

    #[test]
    fn parsea_un_commit_con_todos_sus_campos() {
        let crudo = salida(&[
            "a1b2c3d\u{1f}feat: algo nuevo\u{1f}Alex\u{1f}2026-09-26T12:00:00-06:00\u{1f}f0e0d0c\u{1f}HEAD -> main, origin/main",
        ]);

        let c = &parse_log(&crudo)[0];

        assert_eq!(c.sha, "a1b2c3d");
        assert_eq!(c.mensaje, "feat: algo nuevo");
        assert_eq!(c.autor, "Alex");
        assert_eq!(c.padres, vec!["f0e0d0c"]);
        assert_eq!(c.fecha.to_rfc3339(), "2026-09-26T18:00:00+00:00");
    }

    /// Las etiquetas de rama son lo que hace legible el grafo: sin ellas no
    /// se sabe qué carril es cuál.
    #[test]
    fn extrae_los_nombres_de_rama_y_descarta_el_ruido_de_head() {
        let crudo = salida(&[
            "a1\u{1f}m\u{1f}A\u{1f}2026-09-26T12:00:00Z\u{1f}b2\u{1f}HEAD -> main, origin/main, tag: v1.0",
        ]);

        let refs = &parse_log(&crudo)[0].refs;

        assert!(refs.contains(&"main".to_string()));
        assert!(refs.contains(&"origin/main".to_string()));
        assert!(
            !refs.iter().any(|r| r.contains("HEAD ->")),
            "la flecha de HEAD no es un nombre de rama"
        );
    }

    #[test]
    fn lee_los_coautores_del_cuerpo_del_mensaje() {
        let cuerpo =
            "Explicación del cambio.\n\nCo-Authored-By: Claude Opus 5 <noreply@anthropic.com>";
        assert_eq!(
            parse_coautores(cuerpo),
            vec!["Claude Opus 5 <noreply@anthropic.com>"]
        );
    }

    /// Git no impone mayúsculas en los trailers, y hay herramientas que los
    /// escriben en minúscula.
    #[test]
    fn el_trailer_se_reconoce_sin_importar_las_mayusculas() {
        assert_eq!(parse_coautores("co-authored-by: Alguien <a@b.c>").len(), 1);
        assert_eq!(parse_coautores("CO-AUTHORED-BY: Otro <d@e.f>").len(), 1);
    }

    #[test]
    fn varios_coautores_se_leen_todos() {
        let cuerpo = "x\nCo-Authored-By: Uno <1@x>\nCo-Authored-By: Dos <2@x>";
        assert_eq!(parse_coautores(cuerpo).len(), 2);
    }

    #[test]
    fn un_cuerpo_sin_trailers_no_inventa_coautores() {
        assert!(parse_coautores("solo un mensaje\ncon dos líneas").is_empty());
        assert!(parse_coautores("").is_empty());
    }

    #[test]
    fn un_merge_declara_sus_dos_padres() {
        let crudo = salida(&["m1\u{1f}Merge\u{1f}A\u{1f}2026-09-26T12:00:00Z\u{1f}p1 p2\u{1f}"]);

        assert_eq!(parse_log(&crudo)[0].padres, vec!["p1", "p2"]);
    }

    #[test]
    fn el_primer_commit_del_repo_no_tiene_padres() {
        let crudo = salida(&["raiz\u{1f}inicial\u{1f}A\u{1f}2026-09-26T12:00:00Z\u{1f}\u{1f}"]);

        assert!(parse_log(&crudo)[0].padres.is_empty());
    }

    /// Un mensaje con salto de línea rompería un formato separado por líneas;
    /// por eso los registros van separados por 0x1e y no por `\n`.
    #[test]
    fn un_mensaje_con_salto_de_linea_no_parte_el_registro() {
        let crudo = salida(&[
            "a1\u{1f}titulo\ncuerpo del mensaje\u{1f}A\u{1f}2026-09-26T12:00:00Z\u{1f}b2\u{1f}",
        ]);

        let commits = parse_log(&crudo);
        assert_eq!(commits.len(), 1, "sigue siendo un solo commit");
        assert!(commits[0].mensaje.contains("cuerpo"));
    }

    #[test]
    fn una_salida_vacia_no_produce_commits() {
        assert!(parse_log("").is_empty());
        assert!(parse_log("\u{1e}").is_empty());
    }

    #[test]
    fn lee_el_adelanto_y_retraso_frente_al_remoto() {
        let crudo = "\
  main      a1b2c3d [origin/main: ahead 3, behind 1] mensaje
* feat/x    d4e5f6a [origin/feat/x: ahead 2] otro
  local     0000000 sin remoto
";
        let estado = parse_branch_status(crudo);

        assert_eq!(estado.get("main"), Some(&(3, 1)));
        assert_eq!(estado.get("feat/x"), Some(&(2, 0)));
        assert_eq!(estado.get("local"), None, "sin remoto no hay comparación");
    }

    /// Git marca con `+` las ramas sacadas en un worktree. Sin contemplarlo,
    /// aparecía una rama llamada "+" — visible al instante en un repo con
    /// nueve worktrees.
    #[test]
    fn una_rama_sacada_en_un_worktree_se_lee_por_su_nombre() {
        let estado = parse_branch_status("+ feat/x d4e5f6a [origin/feat/x: ahead 2] otro\n");

        assert_eq!(estado.get("feat/x"), Some(&(2, 0)));
        assert!(!estado.contains_key("+"), "el marcador no es un nombre");
    }

    #[test]
    fn una_rama_al_dia_con_su_remoto_no_reporta_diferencias() {
        let estado = parse_branch_status("  main a1b2c3d [origin/main] mensaje\n");
        assert_eq!(estado.get("main"), Some(&(0, 0)));
    }
}
