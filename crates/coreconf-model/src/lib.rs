//! CORECONF model types and SID file handling.

#[path = "../../../src/error.rs"]
mod error;
#[path = "../../../src/types.rs"]
mod types;
#[path = "../../../src/sid.rs"]
mod sid;
#[path = "../../../src/instance_id.rs"]
pub mod instance_id;
#[path = "../../../src/coreconf.rs"]
mod coreconf;

pub use coreconf::CoreconfModel;
pub use error::{CoreconfError, Result};
pub use instance_id::{Instance, InstancePath};
pub use sid::SidFile;
pub use types::YangType;

/// Composite workspace model placeholder for the new boundary.
#[derive(Debug, Clone, Default)]
pub struct CompositeModel;
