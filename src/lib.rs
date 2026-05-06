//! rust-coreconf - Rust implementation of CORECONF (CoAP Management Interface)
//!
//! This is a facade crate that re-exports types from the workspace members.

pub use coreconf_model::{
    CompositeModel,
    CoreconfError,
    CoreconfModel, // Legacy type for backwards compatibility
    Instance,
    InstancePath,
    Result,
    SidFile,
    YangType,
};
pub use coreconf_runtime::{
    coap_types,
    Backend,
    Datastore,
    MemoryBackend,
    OperationBinding,
    PredicatePath,
    RequestBuilder,
    RequestHandler,
};

