pub mod process;

use crate::error::ProbeError;
use crate::model::ClientKind;
use crate::observation::{Capabilities, SessionObservation};

/// Un recolector por cliente CLI. Lee **una** fuente y emite observaciones
/// crudas sin interpretarlas: la interpretación vive en el motor de estados.
pub trait SessionProbe: Send + Sync {
    fn client(&self) -> ClientKind;

    fn capabilities(&self) -> Capabilities;

    fn observe(&self) -> Result<Vec<SessionObservation>, ProbeError>;
}
