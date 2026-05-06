use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{CoreconfError, Result};
use crate::types::YangType;

#[derive(Debug, Clone)]
pub struct SidFile {
    pub module_name: String,
    pub module_revision: String,
    pub module_prefix: String,
    pub sids: HashMap<String, i64>,
    pub ids: HashMap<i64, String>,
    pub types: HashMap<String, YangType>,
    pub key_mapping: HashMap<i64, Vec<i64>>,
}

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
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        Self::from_json_str(&content)
    }

    pub fn from_json_str(content: &str) -> Result<Self> {
        let raw: RawSidFile = serde_json::from_str(content)?;

        let mut sids = HashMap::with_capacity(raw.item.len());
        let mut ids = HashMap::with_capacity(raw.item.len());
        let mut types = HashMap::with_capacity(raw.item.len());

        for item in &raw.item {
            if let Some(existing_sid) = sids.get(&item.identifier) {
                if existing_sid != &item.sid {
                    return Err(CoreconfError::InvalidSidFile(format!(
                        "identifier conflict for '{}': existing SID {existing_sid}, new SID {}",
                        item.identifier, item.sid
                    )));
                }
            } else {
                sids.insert(item.identifier.clone(), item.sid);
            }

            if let Some(existing_identifier) = ids.get(&item.sid) {
                if existing_identifier != &item.identifier {
                    return Err(CoreconfError::InvalidSidFile(format!(
                        "SID conflict for {}: existing identifier '{}', new identifier '{}'",
                        item.sid, existing_identifier, item.identifier
                    )));
                }
            } else {
                ids.insert(item.sid, item.identifier.clone());
            }

            if let Some(ref type_val) = item.item_type {
                let parsed_type = YangType::from_sid_type(type_val)?;
                if let Some(existing_type) = types.get(&item.identifier) {
                    if existing_type != &parsed_type {
                        return Err(CoreconfError::InvalidSidFile(format!(
                            "type conflict for '{}': existing {existing_type:?}, new {parsed_type:?}",
                            item.identifier
                        )));
                    }
                } else {
                    types.insert(item.identifier.clone(), parsed_type);
                }
            }
        }

        let key_mapping: HashMap<i64, Vec<i64>> = raw
            .key_mapping
            .into_iter()
            .map(|(k, v)| {
                k.parse().map(|sid| (sid, v)).map_err(|_| {
                    CoreconfError::InvalidSidFile(format!(
                        "key-mapping contains invalid list SID '{k}'"
                    ))
                })
            })
            .collect::<Result<_>>()?;

        Ok(SidFile {
            module_prefix: format!("/{}:", raw.module_name),
            module_name: raw.module_name,
            module_revision: raw.module_revision,
            sids,
            ids,
            types,
            key_mapping,
        })
    }

    pub fn get_sid(&self, identifier: &str) -> Option<i64> {
        self.sids.get(identifier).copied()
    }

    pub fn get_identifier(&self, sid: i64) -> Option<&str> {
        self.ids.get(&sid).map(String::as_str)
    }

    pub fn get_type(&self, identifier: &str) -> Option<&YangType> {
        self.types.get(identifier)
    }

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

    #[test]
    fn test_parse_sid_file_rejects_invalid_key_mapping_keys() {
        let err = SidFile::from_json_str(
            r#"{
                "module-name": "example-1",
                "module-revision": "unknown",
                "item": [{"identifier": "example-1", "sid": 60000}],
                "key-mapping": {"invalid": [60001]}
            }"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            crate::error::CoreconfError::InvalidSidFile(message)
                if message.contains("key-mapping")
        ));
    }

    #[test]
    fn test_parse_sid_file_rejects_invalid_enum_metadata() {
        let err = SidFile::from_json_str(
            r#"{
                "module-name": "example-1",
                "module-revision": "unknown",
                "item": [
                    {"identifier": "example-1", "sid": 60000},
                    {
                        "identifier": "/example-1:state",
                        "sid": 60001,
                        "type": {"not-a-number": "up"}
                    }
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            crate::error::CoreconfError::InvalidSidFile(message)
                if message.contains("enumeration")
        ));
    }

    #[test]
    fn test_parse_sid_file_rejects_duplicate_identifier_entries() {
        let err = SidFile::from_json_str(
            r#"{
                "module-name": "example-1",
                "module-revision": "unknown",
                "item": [
                    {"identifier": "example-1", "sid": 60000},
                    {"identifier": "/example-1:state", "sid": 60001, "type": "string"},
                    {"identifier": "/example-1:state", "sid": 60002, "type": "string"}
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            crate::error::CoreconfError::InvalidSidFile(message)
                if message.contains("identifier conflict")
        ));
    }

    #[test]
    fn test_parse_sid_file_accepts_unknown_top_level_type_strings() {
        let sid_file = SidFile::from_json_str(
            r#"{
                "module-name": "example-1",
                "module-revision": "unknown",
                "item": [
                    {"identifier": "example-1", "sid": 60000},
                    {
                        "identifier": "/example-1:address",
                        "sid": 60001,
                        "type": "inet:ipv4-address"
                    }
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap();

        assert_eq!(
            sid_file.get_type("/example-1:address"),
            Some(&YangType::Unknown("inet:ipv4-address".to_string()))
        );
    }

    #[test]
    fn test_parse_sid_file_rejects_unknown_union_member_types() {
        let err = SidFile::from_json_str(
            r#"{
                "module-name": "example-1",
                "module-revision": "unknown",
                "item": [
                    {"identifier": "example-1", "sid": 60000},
                    {
                        "identifier": "/example-1:state",
                        "sid": 60001,
                        "type": ["string", "bogus-type"]
                    }
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            crate::error::CoreconfError::InvalidSidFile(message)
                if message.contains("unknown YANG type")
        ));
    }
}
