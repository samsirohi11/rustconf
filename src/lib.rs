//! rust-coreconf - Rust implementation of CORECONF (CoAP Management Interface)
//!
//! This library provides functionality for converting between JSON and CORECONF
//! (CBOR-encoded YANG data) using YANG SID files, plus CoAP request/response handling for CORECONF operations.
//!
//! # Example
//!
//! ```no_run
//! use rust_coreconf::{CoreconfModel, Datastore, RequestHandler};
//! use rust_coreconf::coap_types::{Request, Method};
//!
//! // Create model from SID file
//! let model = CoreconfModel::new("example.sid").unwrap();
//!
//! // Create datastore with model
//! let datastore = Datastore::new(model);
//!
//! // Create request handler
//! let mut handler = RequestHandler::new(datastore);
//!
//! // Handle incoming requests
//! let request = Request::new(Method::Get);
//! let response = handler.handle(&request);
//! ```

pub mod coap_types;
mod coreconf;
pub mod datastore;
mod error;
pub mod handler;
pub mod instance_id;
pub mod request_builder;
mod sid;
mod types;

pub use coreconf::CoreconfModel;
pub use datastore::Datastore;
pub use error::{CoreconfError, Result};
pub use handler::RequestHandler;
pub use request_builder::RequestBuilder;
pub use sid::SidFile;
pub use types::YangType;
