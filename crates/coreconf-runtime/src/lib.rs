//! CORECONF runtime and request handling.

pub mod error {
    pub use coreconf_model::{CoreconfError, Result};
}

pub mod coreconf {
    pub use coreconf_model::CoreconfModel;
}

pub mod instance_id {
    pub use coreconf_model::instance_id::{
        PathComponent, decode_instances, encode_identifiers, encode_instances,
    };
    pub use coreconf_model::{Instance, InstancePath};
}

#[path = "../../../src/coap_types.rs"]
pub mod coap_types;
#[path = "../../../src/datastore.rs"]
mod datastore;
#[path = "../../../src/handler.rs"]
mod handler;
#[path = "../../../src/request_builder.rs"]
mod request_builder;

pub use datastore::Datastore;
pub use handler::RequestHandler;
pub use request_builder::RequestBuilder;

pub trait Backend {}

#[derive(Debug, Default, Clone)]
pub struct MemoryBackend;

impl Backend for MemoryBackend {}

#[derive(Debug, Clone, Default)]
pub struct OperationBinding;

pub type PredicatePath = coreconf_model::InstancePath;
