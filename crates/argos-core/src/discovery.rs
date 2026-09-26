use crate::error::ProbeError;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub repo_root: PathBuf,
}

/// Parsea la salida de `git worktree list`. El formato es:
/// `<ruta> <sha> [<rama>]`, o `(detached HEAD)` en vez de `[<rama>]`.
/// La ruta puede llevar espacios, así que se corta desde el final.
pub fn parse_worktree_list(output: &str) -> Vec<Worktree> {
    let mut worktrees: Vec<Worktree> = Vec::new();

    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        let Some(open) = line.rfind('[') else {
            // detached HEAD: la ruta es todo menos las dos últimas columnas.
            let Some(sha_start) = line.rfind("(detached HEAD)") else {
                continue;
            };
            let sin_marca = line[..sha_start].trim_end();
            let Some(corte) = sin_marca.rfind(char::is_whitespace) else {
                continue;
            };
            let path = PathBuf::from(sin_marca[..corte].trim_end());
            worktrees.push(Worktree {
                repo_root: path.clone(),
                path,
                branch: None,
            });
            continue;
        };

        let branch = line[open + 1..].trim_end_matches(']').to_string();
        let sin_rama = line[..open].trim_end();
        let Some(corte) = sin_rama.rfind(char::is_whitespace) else {
            continue;
        };
        let path = PathBuf::from(sin_rama[..corte].trim_end());

        worktrees.push(Worktree {
            repo_root: path.clone(),
            path,
            branch: Some(branch),
        });
    }

    // El primero que imprime git es siempre el repo principal.
    if let Some(root) = worktrees.first().map(|w| w.path.clone()) {
        for w in worktrees.iter_mut() {
            w.repo_root = root.clone();
        }
    }

    worktrees
}

/// El worktree más específico que contiene la ruta. El repo principal es
/// prefijo de sus propios worktrees, así que gana el de ruta más larga.
pub fn worktree_containing<'a>(worktrees: &'a [Worktree], path: &Path) -> Option<&'a Worktree> {
    worktrees
        .iter()
        .filter(|w| path.starts_with(&w.path))
        .max_by_key(|w| w.path.as_os_str().len())
}

pub fn discover_worktrees(repo_root: &Path) -> Result<Vec<Worktree>, ProbeError> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo_root)
        .args(["worktree", "list"])
        .output()
        .map_err(|source| ProbeError::Command {
            command: "git worktree list".into(),
            source,
        })?;

    Ok(parse_worktree_list(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Busca repositorios git bajo `search_root` sin descender dentro de ellos.
pub fn find_repos(search_root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let mut walker = walkdir::WalkDir::new(search_root)
        .max_depth(max_depth)
        .into_iter();

    while let Some(entry) = walker.next() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() {
            continue;
        }
        if entry.path().join(".git").exists() {
            repos.push(entry.path().to_path_buf());
            walker.skip_current_dir();
        }
    }

    repos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SALIDA_GIT: &str = "\
/Users/alex/Proyectos/Orion                                      042b8776 [main]
/Users/alex/Proyectos/Orion/.worktrees/adam-slm                  a3989880 [feat/slm-multi-lora]
/Users/alex/Proyectos/Orion/.worktrees/toma-asistencia-qr        c962def0 [hotfix/asistencia-import-materia]
";

    #[test]
    fn extrae_ruta_y_rama_de_cada_worktree() {
        let worktrees = parse_worktree_list(SALIDA_GIT);
        assert_eq!(worktrees.len(), 3);

        assert_eq!(
            worktrees[0].path,
            PathBuf::from("/Users/alex/Proyectos/Orion")
        );
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));

        assert_eq!(
            worktrees[1].path,
            PathBuf::from("/Users/alex/Proyectos/Orion/.worktrees/adam-slm")
        );
        assert_eq!(worktrees[1].branch.as_deref(), Some("feat/slm-multi-lora"));
    }

    #[test]
    fn un_worktree_en_detached_head_no_tiene_rama() {
        let salida = "/Users/alex/Proyectos/X  042b8776 (detached HEAD)\n";
        let worktrees = parse_worktree_list(salida);

        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].branch, None);
    }

    #[test]
    fn una_salida_vacia_no_produce_worktrees() {
        assert!(parse_worktree_list("").is_empty());
    }

    #[test]
    fn encuentra_el_worktree_que_contiene_una_ruta() {
        let worktrees = parse_worktree_list(SALIDA_GIT);
        let ruta = PathBuf::from("/Users/alex/Proyectos/Orion/.worktrees/adam-slm/src/main.rs");

        let encontrado = worktree_containing(&worktrees, &ruta).expect("debe encontrarlo");
        // Debe ganar el más específico, no el repo raíz que también es prefijo.
        assert_eq!(encontrado.branch.as_deref(), Some("feat/slm-multi-lora"));
    }
}
