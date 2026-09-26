use std::path::{Path, PathBuf};

/// Qué proyectos vigila Argos en este ciclo.
///
/// `Projects(vec![])` significa **nada**, no todo: un alcance vacío por
/// descuido volvería a escanear la máquina entera, que es justo lo que esta
/// revisión elimina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Projects(Vec<PathBuf>),
}

impl Scope {
    pub fn all() -> Self {
        Scope::All
    }

    pub fn projects(roots: Vec<PathBuf>) -> Self {
        Scope::Projects(roots)
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Scope::Projects(r) if r.is_empty())
    }

    pub fn roots(&self) -> &[PathBuf] {
        match self {
            Scope::All => &[],
            Scope::Projects(r) => r,
        }
    }

    /// `starts_with` de `Path` compara por segmentos, no por cadena, así que
    /// `/p/orion-old` no cae dentro de `/p/orion`.
    pub fn contains(&self, path: &Path) -> bool {
        match self {
            Scope::All => true,
            Scope::Projects(roots) => roots.iter().any(|r| path.starts_with(r)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn el_alcance_total_acepta_cualquier_ruta() {
        let s = Scope::all();
        assert!(s.contains(Path::new("/lo/que/sea")));
        assert!(!s.is_empty());
    }

    #[test]
    fn un_alcance_acotado_acepta_el_proyecto_y_lo_que_hay_dentro() {
        let s = Scope::projects(vec![PathBuf::from("/p/orion")]);

        assert!(s.contains(Path::new("/p/orion")), "el proyecto mismo");
        assert!(
            s.contains(Path::new("/p/orion/.worktrees/x")),
            "un worktree dentro"
        );
        assert!(!s.contains(Path::new("/p/otro")));
    }

    /// Review Focus #1: `/p/orion-old` no está dentro de `/p/orion`, por más
    /// que la cadena empiece igual.
    #[test]
    fn un_hermano_con_nombre_prefijo_no_cuenta_como_dentro() {
        let s = Scope::projects(vec![PathBuf::from("/p/orion")]);
        assert!(!s.contains(Path::new("/p/orion-old")));
        assert!(!s.contains(Path::new("/p/orion-old/src")));
    }

    /// Review Focus #2: sin selección no se vigila nada. "Vacío" no significa
    /// "todo": eso volvería a escanear la máquina entera por accidente.
    #[test]
    fn un_alcance_vacio_no_acepta_nada() {
        let s = Scope::projects(vec![]);
        assert!(s.is_empty());
        assert!(!s.contains(Path::new("/p/orion")));
    }
}
