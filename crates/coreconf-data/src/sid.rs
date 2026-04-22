use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::Result;
use crate::types::YangType;

/// Represents a parsed YANG SID file
#[derive(Debug, Clone)]
pub struct SidFile {
    /// Module name from the SID file
    pub module_name: String,
    /// Module revision
    pub module_revision: String,
    /// Formatted module name prefix (e.g., "/{module-name}:")
    pub module_prefix: String,
    /// Mapping from identifier path to SID value
    pub sids: HashMap<String, i64>,
    /// Mapping from SID value to identifier path
    pub ids: HashMap<i64, String>,
    /// Mapping from identifier path to YANG type
    pub types: HashMap<String, YangType>,
    /// Key mapping for list entries
    pub key_mapping: HashMap<i64, Vec<i64>>,
}

/// Raw SID file structure for deserialization
#[derive(Debug, Deserialize)]
struct RawSidFile {
    #[serde(rename = "module-name")]
    module_name: String,
    #[serde(rename = "module-revision")]
    module_revision: String,
    #[serde(alias = "items")]
    item: Vec<RawSidItem>,
    #[serde(rename = "key-mapping", default)]
    key_mapping: HashMap<String, Vec<i64>>,
}

#[derive(Debug, Deserialize)]
struct RawSidItem {
    identifier: String,
    sid: i64,
    #[serde(rename = "type")]
    item_type: Option<Value>,
    #[allow(dead_code)]
    namespace: Option<String>,
    #[allow(dead_code)]
    status: Option<String>,
}

impl SidFile {
    /// Parse a SID file from the given path
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        Self::from_json_str(&content)
    }

    /// Parse a SID file from a JSON string
    pub fn from_json_str(content: &str) -> Result<Self> {
        let raw: RawSidFile = serde_json::from_str(content)?;

        let mut sids = HashMap::with_capacity(raw.item.len());
        let mut ids = HashMap::with_capacity(raw.item.len());
        let mut types = HashMap::with_capacity(raw.item.len());

        for item in &raw.item {
            sids.insert(item.identifier.clone(), item.sid);
            ids.insert(item.sid, item.identifier.clone());

            if let Some(ref type_val) = item.item_type {
                let yang_type = YangType::from_sid_type(type_val);
                types.insert(item.identifier.clone(), yang_type);
            }
        }

        // Convert key_mapping keys from string to i64
        let key_mapping: HashMap<i64, Vec<i64>> = raw
            .key_mapping
            .into_iter()
            .filter_map(|(k, v)| k.parse().ok().map(|sid| (sid, v)))
            .collect();

        let module_prefix = format!("/{}:", raw.module_name);

        Ok(SidFile {
            module_name: raw.module_name,
            module_revision: raw.module_revision,
            module_prefix,
            sids,
            ids,
            types,
            key_mapping,
        })
    }

    /// Get SID value for an identifier path
    pub fn get_sid(&self, identifier: &str) -> Option<i64> {
        self.sids.get(identifier).copied()
    }

    /// Get identifier path for a SID value
    pub fn get_identifier(&self, sid: i64) -> Option<&str> {
        self.ids.get(&sid).map(|s| s.as_str())
    }

    /// Get YANG type for an identifier path
    pub fn get_type(&self, identifier: &str) -> Option<&YangType> {
        self.types.get(identifier)
    }

    /// Get keys for a list entry by its SID
    pub fn get_keys(&self, list_sid: i64) -> Option<&Vec<i64>> {
        self.key_mapping.get(&list_sid)
    }
}

impl std::str::FromStr for SidFile {
    type Err = crate::error::CoreconfError;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_json_str(s)
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

    #[test]
    fn test_parse_sid_file() {
        let sid_file: SidFile = SAMPLE_SID.parse().unwrap();

        assert_eq!(sid_file.module_name, "example-1");
        assert_eq!(sid_file.module_revision, "unknown");
        assert_eq!(sid_file.module_prefix, "/example-1:");
    }

    #[test]
    fn test_sid_lookup() {
        let sid_file: SidFile = SAMPLE_SID.parse().unwrap();

        assert_eq!(sid_file.get_sid("/example-1:greeting"), Some(60001));
        assert_eq!(sid_file.get_sid("/example-1:greeting/author"), Some(60002));
        assert_eq!(
            sid_file.get_identifier(60003),
            Some("/example-1:greeting/message")
        );
    }

    #[test]
    fn test_type_lookup() {
        let sid_file: SidFile = SAMPLE_SID.parse().unwrap();

        assert_eq!(
            sid_file.get_type("/example-1:greeting/author"),
            Some(&YangType::String)
        );
    }
}
