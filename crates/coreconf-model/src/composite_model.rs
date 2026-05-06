use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::error::{CoreconfError, Result};
use crate::sid_file::SidFile;
use crate::types::{YangType, cast_from_coreconf, cast_to_coreconf};

#[derive(Debug, Clone)]
pub struct CompositeModel {
    pub sid_files: Vec<SidFile>,
    pub sids: HashMap<String, i64>,
    pub ids: HashMap<i64, String>,
    pub types: HashMap<String, YangType>,
    pub key_mapping: HashMap<i64, Vec<i64>>,
}

impl CompositeModel {
    pub fn from_sid_strings(contents: &[&str]) -> Result<Self> {
        let sid_files = contents
            .iter()
            .map(|content| SidFile::from_json_str(content))
            .collect::<Result<Vec<_>>>()?;
        Self::from_sid_files(sid_files)
    }

    pub fn from_sid_files(sid_files: Vec<SidFile>) -> Result<Self> {
        let mut sids = HashMap::new();
        let mut ids = HashMap::new();
        let mut types = HashMap::new();
        let mut key_mapping = HashMap::new();

        for sid_file in &sid_files {
            for (identifier, sid) in &sid_file.sids {
                if let Some(existing_sid) = sids.get(identifier) {
                    if existing_sid != sid {
                        return Err(CoreconfError::InvalidSidFile(format!(
                            "identifier conflict for '{identifier}': existing SID {existing_sid}, new SID {sid}"
                        )));
                    }
                } else {
                    sids.insert(identifier.clone(), *sid);
                }
            }

            for (sid, identifier) in &sid_file.ids {
                if let Some(existing_identifier) = ids.get(sid) {
                    if existing_identifier != identifier {
                        return Err(CoreconfError::InvalidSidFile(format!(
                            "SID conflict for {sid}: existing identifier '{existing_identifier}', new identifier '{identifier}'"
                        )));
                    }
                } else {
                    ids.insert(*sid, identifier.clone());
                }
            }

            for (identifier, yang_type) in &sid_file.types {
                if let Some(existing_type) = types.get(identifier) {
                    if existing_type != yang_type {
                        return Err(CoreconfError::InvalidSidFile(format!(
                            "type conflict for '{identifier}': existing {existing_type:?}, new {yang_type:?}"
                        )));
                    }
                } else {
                    types.insert(identifier.clone(), yang_type.clone());
                }
            }

            for (sid, keys) in &sid_file.key_mapping {
                if let Some(existing_keys) = key_mapping.get(sid) {
                    if existing_keys != keys {
                        return Err(CoreconfError::InvalidSidFile(format!(
                            "key-mapping conflict for SID {sid}: existing {existing_keys:?}, new {keys:?}"
                        )));
                    }
                } else {
                    key_mapping.insert(*sid, keys.clone());
                }
            }
        }

        Ok(Self {
            sid_files,
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

    pub fn identifier_value_to_sid_value(&self, json_data: Value) -> Result<Value> {
        self.process_value_for_sid(&json_data, None, 0)
    }

    pub fn sid_value_to_identifier_value(&self, coreconf_data: Value) -> Result<Value> {
        self.process_value_for_identifier(&coreconf_data, 0, None)
    }

    fn process_value_for_sid(
        &self,
        value: &Value,
        current_path: Option<&str>,
        parent_sid: i64,
    ) -> Result<Value> {
        match value {
            Value::Object(map) => {
                let mut new_map = Map::new();
                for (key, v) in map {
                    let qualified_path = match current_path {
                        Some(path) => format!("{path}/{key}"),
                        None => format!("/{key}"),
                    };

                    let child_sid = self
                        .get_sid(&qualified_path)
                        .ok_or_else(|| CoreconfError::SidNotFound(qualified_path.clone()))?;
                    let sid_delta = child_sid - parent_sid;
                    let processed =
                        self.process_value_for_sid(v, Some(&qualified_path), child_sid)?;
                    new_map.insert(sid_delta.to_string(), processed);
                }
                Ok(Value::Object(new_map))
            }
            Value::Array(arr) => {
                let mut new_arr = Vec::with_capacity(arr.len());
                for elem in arr {
                    new_arr.push(self.process_value_for_sid(elem, current_path, parent_sid)?);
                }
                Ok(Value::Array(new_arr))
            }
            _ => {
                if let Some(path) = current_path
                    && let Some(yang_type) = self.get_type(path)
                {
                    let sid_lookup = |id: &str| self.get_sid(id);
                    return cast_to_coreconf(value, yang_type, Some(&sid_lookup));
                }
                Ok(value.clone())
            }
        }
    }

    fn process_value_for_identifier(
        &self,
        value: &Value,
        delta: i64,
        current_path: Option<&str>,
    ) -> Result<Value> {
        match value {
            Value::Object(map) => {
                let mut new_map = Map::new();
                for (key, v) in map {
                    let key_delta: i64 = key.parse().map_err(|_| {
                        CoreconfError::TypeConversion(format!("invalid SID key: {key}"))
                    })?;
                    let sid = key_delta + delta;
                    let identifier = self
                        .get_identifier(sid)
                        .ok_or(CoreconfError::IdentifierNotFound(sid))?;
                    let leaf_name = identifier.split('/').next_back().unwrap_or(identifier);
                    let processed = self.process_value_for_identifier(v, sid, Some(identifier))?;
                    new_map.insert(leaf_name.to_string(), processed);
                }
                Ok(Value::Object(new_map))
            }
            Value::Array(arr) => {
                let mut new_arr = Vec::with_capacity(arr.len());
                for elem in arr {
                    new_arr.push(self.process_value_for_identifier(elem, delta, current_path)?);
                }
                Ok(Value::Array(new_arr))
            }
            _ => {
                if let Some(path) = current_path
                    && let Some(yang_type) = self.get_type(path)
                {
                    let id_lookup = |sid: i64| self.get_identifier(sid).map(str::to_string);
                    let module_name = self.module_name_for_identifier(path).unwrap_or_default();
                    return cast_from_coreconf(value, yang_type, Some(&id_lookup), module_name);
                }
                Ok(value.clone())
            }
        }
    }

    fn module_name_for_identifier(&self, identifier: &str) -> Option<&str> {
        self.sid_files
            .iter()
            .find(|sid_file| sid_file.get_sid(identifier).is_some())
            .map(|sid_file| sid_file.module_name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::CompositeModel;

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
    fn test_identifier_value_to_sid_value() {
        let model = CompositeModel::from_sid_strings(&[SAMPLE_SID]).unwrap();
        let value: Value = serde_json::from_str(SAMPLE_JSON).unwrap();

        let converted = model.identifier_value_to_sid_value(value).unwrap();

        assert!(converted.is_object());
    }

    #[test]
    fn test_roundtrip_value_conversion() {
        let model = CompositeModel::from_sid_strings(&[SAMPLE_SID]).unwrap();
        let value: Value = serde_json::from_str(SAMPLE_JSON).unwrap();

        let sid_value = model.identifier_value_to_sid_value(value).unwrap();
        let json_value = model.sid_value_to_identifier_value(sid_value).unwrap();

        assert_eq!(json_value["example-1:greeting"]["author"], "Obi");
        assert_eq!(json_value["example-1:greeting"]["message"], "Hello there!");
    }
}
