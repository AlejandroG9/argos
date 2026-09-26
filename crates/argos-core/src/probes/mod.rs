pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod gemini;
pub mod process;

use crate::error::ProbeError;
use crate::model::ClientKind;
use crate::observation::{Capabilities, SessionObservation};
use crate::scope::Scope;

/// Un recolector por cliente CLI. Lee **una** fuente y emite observaciones
/// crudas sin interpretarlas: la interpretación vive en el motor de estados.
pub trait SessionProbe: Send + Sync {
    fn client(&self) -> ClientKind;

    fn capabilities(&self) -> Capabilities;

    /// `scope` acota qué proyectos interesan. Cada implementación debe
    /// aplicarlo **lo antes que su formato permita**, no al final.
    fn observe(&self, scope: &Scope) -> Result<Vec<SessionObservation>, ProbeError>;
}
