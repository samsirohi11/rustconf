//! Core CORECONF conversion logic

use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{CoreconfError, Result};
use crate::sid::SidFile;
use crate::types::{cast_from_coreconf, cast_to_coreconf};

/// CORECONF model for JSON/CBOR conversion
#[derive(Debug, Clone)]
pub struct CoreconfModel {
    /// Parsed SID file
    pub sid_file: SidFile,
}

impl CoreconfModel {
    /// Create a new CORECONF model from a SID file path
    pub fn new(sid_path: impl AsRef<Path>) -> Result<Self> {
        let sid_file = SidFile::from_file(sid_path)?;
        Ok(Self { sid_file })
    }

    /// Create a new CORECONF model from a SID file string
    pub fn from_str(sid_content: &str) -> Result<Self> {
        let sid_file = SidFile::from_str(sid_content)?;
        Ok(Self { sid_file })
    }

    /// Convert JSON string to CORECONF (CBOR bytes)
    pub fn to_coreconf(&self, json_data: &str) -> Result<Vec<u8>> {
        let py_dict: Value = serde_json::from_str(json_data)?;
        let coreconf_value = self.lookup_sid(py_dict)?;

        // Encode to CBOR
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&coreconf_value, &mut cbor_bytes)
            .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;

        Ok(cbor_bytes)
    }

    /// Convert JSON file to CORECONF (CBOR bytes)
    pub fn file_to_coreconf(&self, json_path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let content = std::fs::read_to_string(json_path)?;
        self.to_coreconf(&content)
    }

    /// Convert CORECONF (CBOR bytes) to JSON string
    pub fn to_json(&self, cbor_data: &[u8]) -> Result<String> {
        let value = self.to_value(cbor_data)?;
        Ok(serde_json::to_string(&value)?)
    }

    /// Convert CORECONF (CBOR bytes) to JSON string (pretty printed)
    pub fn to_json_pretty(&self, cbor_data: &[u8]) -> Result<String> {
        let value = self.to_value(cbor_data)?;
        Ok(serde_json::to_string_pretty(&value)?)
    }

    /// Convert CORECONF (CBOR bytes) to serde_json::Value
    pub fn to_value(&self, cbor_data: &[u8]) -> Result<Value> {
        let coreconf_value: Value = ciborium::from_reader(cbor_data)
            .map_err(|e| CoreconfError::CborDecode(e.to_string()))?;
        self.lookup_identifier(coreconf_value)
    }

    /// Transform JSON keys to SID deltas
    fn lookup_sid(&self, json_data: Value) -> Result<Value> {
        self.process_value_for_sid(&json_data, &self.sid_file.module_prefix, 0)
    }

    fn process_value_for_sid(&self, value: &Value, path: &str, parent_sid: i64) -> Result<Value> {
        match value {
            Value::Object(map) => {
                let mut new_map = Map::new();
                for (key, v) in map {
                    // Build the qualified path:
                    // - At top level, path is like "/example-1:" and key is "example-1:greeting"
                    //   so we need to just use the key directly as the identifier
                    // - At nested level, path is like "/example-1:greeting" and key is "author"
                    //   so we need "{path}/{key}"
                    let qualified_path = if path.ends_with(':') {
                        // Top level: key already contains the module prefix
                        format!("/{}", key)
                    } else {
                        // Nested level: append key to path
                        format!("{}/{}", path, key)
                    };

                    let child_sid = self
                        .sid_file
                        .get_sid(&qualified_path)
                        .ok_or_else(|| CoreconfError::SidNotFound(qualified_path.clone()))?;
                    let sid_delta = child_sid - parent_sid;

                    let processed = self.process_value_for_sid(v, &qualified_path, child_sid)?;
                    new_map.insert(sid_delta.to_string(), processed);
                }
                Ok(Value::Object(new_map))
            }
            Value::Array(arr) => {
                let mut new_arr = Vec::new();
                for elem in arr {
                    let processed = self.process_value_for_sid(elem, path, parent_sid)?;
                    new_arr.push(processed);
                }
                Ok(Value::Array(new_arr))
            }
            _ => {
                // Leaf value - apply type casting
                if let Some(yang_type) = self.sid_file.get_type(path) {
                    let sid_lookup = |id: &str| self.sid_file.get_sid(id);
                    cast_to_coreconf(value, yang_type, Some(&sid_lookup))
                } else {
                    Ok(value.clone())
                }
            }
        }
    }

    /// Transform SID deltas back to JSON identifiers
    fn lookup_identifier(&self, coreconf_data: Value) -> Result<Value> {
        self.process_value_for_identifier(&coreconf_data, 0, "/")
    }

    fn process_value_for_identifier(&self, value: &Value, delta: i64, path: &str) -> Result<Value> {
        match value {
            Value::Object(map) => {
                let mut new_map = Map::new();
                for (key, v) in map {
                    let key_delta: i64 = key.parse().map_err(|_| {
                        CoreconfError::TypeConversion(format!("invalid SID key: {}", key))
                    })?;
                    let sid = key_delta + delta;
                    let identifier = self
                        .sid_file
                        .get_identifier(sid)
                        .ok_or(CoreconfError::IdentifierNotFound(sid))?;

                    // Get the leaf name (last component of path)
                    let leaf_name = identifier.split('/').next_back().unwrap_or(identifier);

                    let processed = self.process_value_for_identifier(v, sid, identifier)?;
                    new_map.insert(leaf_name.to_string(), processed);
                }
                Ok(Value::Object(new_map))
            }
            Value::Array(arr) => {
                let mut new_arr = Vec::new();
                for elem in arr {
                    let processed = self.process_value_for_identifier(elem, delta, path)?;
                    new_arr.push(processed);
                }
                Ok(Value::Array(new_arr))
            }
            _ => {
                // Leaf value - apply type casting
                if let Some(yang_type) = self.sid_file.get_type(path) {
                    let id_lookup =
                        |sid: i64| self.sid_file.get_identifier(sid).map(|s| s.to_string());
                    cast_from_coreconf(
                        value,
                        yang_type,
                        Some(&id_lookup),
                        &self.sid_file.module_name,
                    )
                } else {
                    Ok(value.clone())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SID: &str = r#"{
        "assignment-range": [{"entry-point": 60000, "size": 10}],
        "module-name": "example-1",
        "module-revision": "unknown",
        "item": [
            {"namespace": "module", "identifier": "example-1", "status": "unstable", "sid": 60000},
            {"namespace": "data", "identifier": "/example-1:greeting", "status": "unstable", "sid": 60001},
            {"namespace": "data", "identifier": "/example-1:greeting/author", "status": "unstable", "sid": 60002, "type": "string"},
            {"namespace": "data", "identifier": "/example-1:greeting/message", "status": "unstable", "sid": 60003, "type": "string"}
        ],
        "key-mapping": {}
    }"#;

    const SAMPLE_JSON: &str =
        r#"{"example-1:greeting": {"author": "Obi", "message": "Hello there!"}}"#;

    #[test]
    fn test_to_coreconf() {
        let model = CoreconfModel::from_str(SAMPLE_SID).unwrap();
        let cbor = model.to_coreconf(SAMPLE_JSON).unwrap();

        // CBOR should be non-empty
        assert!(!cbor.is_empty());
        println!("CBOR hex: {}", hex::encode(&cbor));
    }

    #[test]
    fn test_roundtrip() {
        let model = CoreconfModel::from_str(SAMPLE_SID).unwrap();

        // Encode
        let cbor = model.to_coreconf(SAMPLE_JSON).unwrap();

        // Decode
        let json = model.to_json(&cbor).unwrap();
        let decoded: Value = serde_json::from_str(&json).unwrap();

        // Verify structure
        assert!(decoded.is_object());
        let greeting = &decoded["example-1:greeting"];
        assert_eq!(greeting["author"], "Obi");
        assert_eq!(greeting["message"], "Hello there!");
    }
}
