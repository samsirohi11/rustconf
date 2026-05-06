//! CORECONF model types and SID file handling

use std::fmt;
use std::str::FromStr;

/// A composite model that can aggregate multiple SID files
pub struct CompositeModel;

/// Legacy CoreconfModel type (for backwards compatibility during migration)
#[derive(Clone)]
pub struct CoreconfModel;

impl FromStr for CoreconfModel {
    type Err = CoreconfError;

    fn from_str(_s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(CoreconfModel)
    }
}

impl CoreconfModel {
    pub fn to_coreconf(&self, _json: &str) -> Result<Vec<u8>> {
        Ok(vec![])
    }

    pub fn to_json(&self, _cbor: &[u8]) -> Result<String> {
        Ok(String::new())
    }
}

/// CORECONF error type
#[derive(Debug)]
pub struct CoreconfError;

impl fmt::Display for CoreconfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CoreconfError")
    }
}

impl std::error::Error for CoreconfError {}

/// Result type for CORECONF operations
pub type Result<T> = std::result::Result<T, CoreconfError>;

/// A YANG instance
pub struct Instance;

/// Path to a YANG instance
pub struct InstancePath;

/// SID file representation
pub struct SidFile;

/// YANG type enumeration
pub enum YangType {
    String,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Boolean,
    Decimal64,
    Empty,
    Binary,
    Identityref,
    Union,
}
