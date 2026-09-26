use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("no se encontró la fuente de datos en {0}")]
    SourceMissing(PathBuf),

    #[error("no se pudo leer {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("fallo al ejecutar `{command}`: {source}")]
    Command {
        command: String,
        source: std::io::Error,
    },

    #[error("formato inesperado en {path}: {detail}")]
    Format { path: PathBuf, detail: String },
}
