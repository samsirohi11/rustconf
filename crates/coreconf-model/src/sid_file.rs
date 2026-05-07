use std::collections::HashMap;
use std::fs;
use std::path::Path;

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

/// Extract a string field from a JSON object. Returns an error if the key is missing
/// or the value is not a string.
fn extract_string(obj: &serde_json::Map<String, Value>, key: &str) -> Result<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CoreconfError::InvalidSidFile(format!("missing or invalid '{key}' field")))
}

/// Parsed SID item from the JSON representation.
struct ParsedItem {
    identifier: String,
    sid_value: Value,
    item_type: Option<Value>,
    namespace: Option<String>,
}

/// Extract items from the "item" or "items" array.
fn extract_items(sid_data: &serde_json::Map<String, Value>) -> Result<Vec<ParsedItem>> {
    let items_value = sid_data
        .get("item")
        .or_else(|| sid_data.get("items"))
        .ok_or_else(|| CoreconfError::InvalidSidFile("missing 'item' or 'items' array".into()))?;

    let arr = items_value.as_array().ok_or_else(|| {
        CoreconfError::InvalidSidFile("'item'/'items' must be a JSON array".into())
    })?;

    arr.iter()
        .map(|entry| {
            let obj = entry.as_object().ok_or_else(|| {
                CoreconfError::InvalidSidFile("each item entry must be a JSON object".into())
            })?;
            Ok(ParsedItem {
                identifier: extract_string(obj, "identifier")?,
                sid_value: obj
                    .get("sid")
                    .cloned()
                    .ok_or_else(|| CoreconfError::InvalidSidFile("item missing 'sid'".into()))?,
                item_type: obj.get("type").cloned(),
                namespace: obj
                    .get("namespace")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

/// Extract key-mapping from the SID file data.
fn extract_key_mapping(sid_data: &serde_json::Map<String, Value>) -> HashMap<String, Vec<Value>> {
    sid_data
        .get("key-mapping")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_array().map(|arr| (k.clone(), arr.clone())))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a SID value that may be a JSON number or a JSON string.
fn parse_sid_value(value: &Value) -> Result<i64> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| CoreconfError::InvalidSidFile(format!("SID value is not an i64: {n}"))),
        Value::String(s) => s.parse::<i64>().map_err(|_| {
            CoreconfError::InvalidSidFile(format!("SID string is not a valid i64: '{s}'"))
        }),
        _ => Err(CoreconfError::InvalidSidFile(format!(
            "SID value must be a number or string, got: {value:?}"
        ))),
    }
}

impl SidFile {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        Self::from_json_str(&content)
    }

    pub fn from_json_str(content: &str) -> Result<Self> {
        let mut root: Value = serde_json::from_str(content)?;

        // Unwrap RFC 9595 envelope: {"ietf-sid-file:sid-file": {...}} -> {...}
        if let Value::Object(ref map) = root {
            if map.len() == 1 {
                let envelope_key = map.keys().next().unwrap();
                if envelope_key.ends_with("sid-file") {
                    root = map[envelope_key].clone();
                }
            }
        }

        let sid_data = root.as_object().ok_or_else(|| {
            CoreconfError::InvalidSidFile("SID file root must be a JSON object".into())
        })?;

        let module_name = extract_string(sid_data, "module-name")?;
        let module_revision =
            extract_string(sid_data, "module-revision").unwrap_or_else(|_| "unknown".into());

        let items = extract_items(sid_data)?;
        let raw_key_mapping = extract_key_mapping(sid_data);

        let mut sids = HashMap::with_capacity(items.len());
        let mut ids = HashMap::with_capacity(items.len());
        let mut types = HashMap::with_capacity(items.len());

        for item in items {
            let sid = parse_sid_value(&item.sid_value)?;
            let identifier = item.identifier;

            // Identity namespace items are stored as module_name:identity (no leading /).
            // Data/module/rpc/notification items keep their full YANG path.
            let storage_key = if item.namespace.as_deref() == Some("identity") {
                // identity namespace: e.g. "solar-radiation" -> "coreconf-m2m:solar-radiation"
                format!("{module_name}:{identifier}")
            } else {
                identifier.clone()
            };

            if let Some(existing_sid) = sids.get(&storage_key) {
                if existing_sid != &sid {
                    return Err(CoreconfError::InvalidSidFile(format!(
                        "identifier conflict for '{storage_key}': existing SID {existing_sid}, new SID {sid}"
                    )));
                }
            } else {
                sids.insert(storage_key.clone(), sid);
            }

            if let Some(existing_identifier) = ids.get(&sid) {
                if existing_identifier != &storage_key {
                    return Err(CoreconfError::InvalidSidFile(format!(
                        "SID conflict for {sid}: existing identifier '{existing_identifier}', new identifier '{storage_key}'"
                    )));
                }
            } else {
                ids.insert(sid, storage_key.clone());
            }

            if let Some(ref type_val) = item.item_type {
                let parsed_type = YangType::from_sid_type(type_val)?;
                if let Some(existing_type) = types.get(&storage_key) {
                    if existing_type != &parsed_type {
                        return Err(CoreconfError::InvalidSidFile(format!(
                            "type conflict for '{storage_key}': existing {existing_type:?}, new {parsed_type:?}"
                        )));
                    }
                } else {
                    types.insert(storage_key.clone(), parsed_type);
                }
            }
        }

        let mut key_mapping: HashMap<i64, Vec<i64>> = HashMap::new();
        for (key, values) in raw_key_mapping {
            let list_sid: i64 = key.parse().map_err(|_| {
                CoreconfError::InvalidSidFile(format!(
                    "key-mapping contains invalid list SID '{key}'"
                ))
            })?;
            let key_sids: Vec<i64> = values.iter().map(parse_sid_value).collect::<Result<_>>()?;
            key_mapping.insert(list_sid, key_sids);
        }

        Ok(SidFile {
            module_prefix: format!("/{module_name}:"),
            module_name,
            module_revision,
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

    // --- Parser format compatibility tests ---

    #[test]
    fn parses_rfc9595_wrapper_envelope() {
        let sid_file = SidFile::from_json_str(
            r#"{
                "ietf-sid-file:sid-file": {
                    "module-name": "test-mod",
                    "module-revision": "2026-01-01",
                    "item": [
                        {"identifier": "test-mod", "sid": "50000"},
                        {"identifier": "/test-mod:leaf", "sid": 50001}
                    ],
                    "key-mapping": {}
                }
            }"#,
        )
        .unwrap();

        assert_eq!(sid_file.module_name, "test-mod");
        assert_eq!(sid_file.get_sid("test-mod"), Some(50000));
        assert_eq!(sid_file.get_sid("/test-mod:leaf"), Some(50001));
    }

    #[test]
    fn parses_string_sid_values() {
        let sid_file = SidFile::from_json_str(
            r#"{
                "module-name": "test-mod",
                "module-revision": "unknown",
                "item": [
                    {"identifier": "test-mod", "sid": "60000"},
                    {"identifier": "/test-mod:data", "sid": "60001", "type": "string"}
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap();

        assert_eq!(sid_file.get_sid("test-mod"), Some(60000));
        assert_eq!(sid_file.get_sid("/test-mod:data"), Some(60001));
        assert_eq!(sid_file.get_type("/test-mod:data"), Some(&YangType::String));
    }

    #[test]
    fn parses_items_alias() {
        let sid_file = SidFile::from_json_str(
            r#"{
                "module-name": "test-mod",
                "module-revision": "unknown",
                "items": [
                    {"identifier": "test-mod", "sid": 50000}
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap();

        assert_eq!(sid_file.get_sid("test-mod"), Some(50000));
    }

    #[test]
    fn parses_identity_namespace_items() {
        let sid_file = SidFile::from_json_str(
            r#"{
                "module-name": "my-module",
                "module-revision": "2026-01-01",
                "item": [
                    {"namespace": "module", "identifier": "my-module", "sid": 60000},
                    {"namespace": "identity", "identifier": "solar-radiation", "sid": 60001},
                    {"namespace": "identity", "identifier": "air-temperature", "sid": 60002},
                    {"namespace": "data", "identifier": "/my-module:reading", "sid": 60003},
                    {"namespace": "data", "identifier": "/my-module:reading/type", "sid": 60004, "type": "identityref"}
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap();

        // Identity items stored as module_name:identity_name
        assert_eq!(sid_file.get_sid("my-module:solar-radiation"), Some(60001));
        assert_eq!(sid_file.get_sid("my-module:air-temperature"), Some(60002));
        // Data items keep their full path
        assert_eq!(sid_file.get_sid("/my-module:reading"), Some(60003));
        assert_eq!(
            sid_file.get_identifier(60001),
            Some("my-module:solar-radiation")
        );
    }

    #[test]
    fn parses_string_assignment_range() {
        let sid_file = SidFile::from_json_str(
            r#"{
                "assignment-range": [{"entry-point": "100000", "size": "400"}],
                "module-name": "test-mod",
                "module-revision": "unknown",
                "item": [
                    {"identifier": "test-mod", "sid": "100000"}
                ],
                "key-mapping": {}
            }"#,
        )
        .unwrap();

        assert_eq!(sid_file.module_name, "test-mod");
        assert_eq!(sid_file.get_sid("test-mod"), Some(100000));
    }

    #[test]
    fn parses_string_key_mapping_values() {
        let sid_file = SidFile::from_json_str(
            r#"{
                "module-name": "test-mod",
                "module-revision": "unknown",
                "item": [
                    {"identifier": "test-mod", "sid": 60000},
                    {"identifier": "/test-mod:list", "sid": 60001},
                    {"identifier": "/test-mod:list/key-a", "sid": "60002"},
                    {"identifier": "/test-mod:list/key-b", "sid": 60003}
                ],
                "key-mapping": {
                    "60001": ["60002", "60003"]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(sid_file.get_keys(60001), Some(&vec![60002, 60003]));
    }
}
