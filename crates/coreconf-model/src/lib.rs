use std::path::Path;

use serde_json::Value;

pub mod codec;
pub mod composite_model;
pub mod error;
pub mod instance_id;
pub mod sid_file;
pub mod types;

pub use codec::{decode_cbor_to_json, encode_json_to_cbor};
pub use composite_model::CompositeModel;
pub use error::{CoreconfError, Result};
pub use instance_id::{Instance, InstancePath};
pub use sid_file::SidFile;
pub use types::YangType;

#[derive(Debug, Clone)]
pub struct CoreconfModel {
    pub sid_file: SidFile,
    composite: CompositeModel,
}

impl CoreconfModel {
    pub fn new(sid_path: impl AsRef<Path>) -> Result<Self> {
        Self::from_sid_file(SidFile::from_file(sid_path)?)
    }

    pub fn from_sid_str(sid_content: &str) -> Result<Self> {
        Self::from_sid_file(SidFile::from_json_str(sid_content)?)
    }

    pub fn to_coreconf(&self, json_data: &str) -> Result<Vec<u8>> {
        encode_json_to_cbor(&self.composite, json_data)
    }

    pub fn file_to_coreconf(&self, json_path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let content = std::fs::read_to_string(json_path)?;
        self.to_coreconf(&content)
    }

    pub fn to_json(&self, cbor_data: &[u8]) -> Result<String> {
        decode_cbor_to_json(&self.composite, cbor_data)
    }

    pub fn to_json_pretty(&self, cbor_data: &[u8]) -> Result<String> {
        let value = self.to_value(cbor_data)?;
        Ok(serde_json::to_string_pretty(&value)?)
    }

    pub fn to_value(&self, cbor_data: &[u8]) -> Result<Value> {
        let coreconf_value: Value = ciborium::from_reader(cbor_data)
            .map_err(|e| CoreconfError::CborDecode(e.to_string()))?;
        self.composite.sid_value_to_identifier_value(coreconf_value)
    }

    pub fn composite_model(&self) -> &CompositeModel {
        &self.composite
    }

    fn from_sid_file(sid_file: SidFile) -> Result<Self> {
        let composite = CompositeModel::from_sid_files(vec![sid_file.clone()])?;
        Ok(Self {
            sid_file,
            composite,
        })
    }
}

impl std::str::FromStr for CoreconfModel {
    type Err = CoreconfError;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_sid_str(s)
    }
}
