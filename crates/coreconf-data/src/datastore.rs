//! Unified YANG Datastore management
//!
//! The Datastore stores and manages YANG data instances,
//! supporting get/set/delete operations used by CORECONF handlers.

use crate::coreconf::CoreconfModel;
use crate::error::{CoreconfError, Result};
use crate::instance_id::InstancePath;
use serde_json::{Map, Value};

/// Unified datastore for YANG data
#[derive(Debug, Clone)]
pub struct Datastore {
    /// The CORECONF model (SID file)
    model: CoreconfModel,
    /// The current data tree
    data: Value,
}

impl Datastore {
    /// Create a new empty datastore
    pub fn new(model: CoreconfModel) -> Self {
        Self {
            model,
            data: Value::Object(Map::new()),
        }
    }

    /// Create a datastore with initial data (JSON)
    pub fn with_data(model: CoreconfModel, data: Value) -> Self {
        Self { model, data }
    }

    /// Create a datastore from JSON string
    pub fn from_json(model: CoreconfModel, json: &str) -> Result<Self> {
        let data: Value = serde_json::from_str(json)?;
        Ok(Self::with_data(model, data))
    }

    /// Get the CORECONF model
    pub fn model(&self) -> &CoreconfModel {
        &self.model
    }

    /// Get the entire data tree
    pub fn get_all(&self) -> &Value {
        &self.data
    }

    /// Get the entire data tree as CBOR bytes
    pub fn get_all_cbor(&self) -> Result<Vec<u8>> {
        self.model.to_coreconf(&self.data.to_string())
    }

    /// Get a value at the given SID
    pub fn get_by_sid(&self, sid: i64) -> Result<Option<Value>> {
        let identifier = self
            .model
            .sid_file
            .get_identifier(sid)
            .ok_or(CoreconfError::IdentifierNotFound(sid))?;
        self.get_by_path(identifier)
    }

    /// Get a value by YANG path (e.g., "/example:container/leaf")
    pub fn get_by_path(&self, path: &str) -> Result<Option<Value>> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if parts.is_empty() {
            return Ok(Some(self.data.clone()));
        }

        let mut current = &self.data;
        for part in &parts {
            // Handle module prefix (e.g., "example:greeting")
            let key = part.to_string();

            match current.get(&key) {
                Some(v) => current = v,
                None => {
                    // Try without module prefix for nested nodes
                    let leaf_name = part.split(':').next_back().unwrap_or(part);
                    match current.get(leaf_name) {
                        Some(v) => current = v,
                        None => return Ok(None),
                    }
                }
            }
        }

        Ok(Some(current.clone()))
    }

    /// Get value using instance path
    pub fn get(&self, path: &InstancePath) -> Result<Option<Value>> {
        if let Some(sid) = path.absolute_sid() {
            self.get_by_sid(sid)
        } else if path.is_empty() {
            Ok(Some(self.data.clone()))
        } else {
            Ok(None)
        }
    }

    /// Set a value at the given SID
    pub fn set_by_sid(&mut self, sid: i64, value: Value) -> Result<()> {
        let identifier = self
            .model
            .sid_file
            .get_identifier(sid)
            .ok_or(CoreconfError::IdentifierNotFound(sid))?
            .to_string();
        self.set_by_path(&identifier, value)
    }

    /// Set a value by YANG path
    pub fn set_by_path(&mut self, path: &str, value: Value) -> Result<()> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if parts.is_empty() {
            self.data = value;
            return Ok(());
        }

        // Navigate to parent and set the leaf
        let mut current = &mut self.data;

        for (i, part) in parts.iter().enumerate() {
            let key = part.to_string();
            let is_last = i == parts.len() - 1;

            if is_last {
                // Set the value
                if let Value::Object(map) = current {
                    map.insert(key, value.clone());
                    return Ok(());
                }
            } else {
                // Navigate or create intermediate containers
                if let Value::Object(map) = current {
                    current = map.entry(key).or_insert_with(|| Value::Object(Map::new()));
                }
            }
        }

        Ok(())
    }

    /// Set value using instance path
    pub fn set(&mut self, path: &InstancePath, value: Value) -> Result<()> {
        if let Some(sid) = path.absolute_sid() {
            self.set_by_sid(sid, value)
        } else if path.is_empty() {
            self.data = value;
            Ok(())
        } else {
            Err(CoreconfError::ResourceNotFound("invalid path".into()))
        }
    }

    /// Delete a value at the given SID
    pub fn delete_by_sid(&mut self, sid: i64) -> Result<bool> {
        let identifier = self
            .model
            .sid_file
            .get_identifier(sid)
            .ok_or(CoreconfError::IdentifierNotFound(sid))?
            .to_string();
        self.delete_by_path(&identifier)
    }

    /// Delete a value by YANG path
    pub fn delete_by_path(&mut self, path: &str) -> Result<bool> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if parts.is_empty() {
            self.data = Value::Object(Map::new());
            return Ok(true);
        }

        // Navigate to parent and delete the leaf
        let mut current = &mut self.data;

        for (i, part) in parts.iter().enumerate() {
            let key = part.to_string();
            let is_last = i == parts.len() - 1;

            if is_last {
                if let Value::Object(map) = current {
                    return Ok(map.remove(&key).is_some());
                }
            } else if let Value::Object(map) = current {
                if let Some(v) = map.get_mut(&key) {
                    current = v;
                } else {
                    return Ok(false);
                }
            }
        }

        Ok(false)
    }

    /// Delete using instance path
    pub fn delete(&mut self, path: &InstancePath) -> Result<bool> {
        if let Some(sid) = path.absolute_sid() {
            self.delete_by_sid(sid)
        } else if path.is_empty() {
            self.data = Value::Object(Map::new());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Apply multiple changes (for iPATCH)
    /// Each change is (path, Option<Value>) where None means delete
    pub fn apply_changes(&mut self, changes: &[(String, Option<Value>)]) -> Result<()> {
        for (path, value) in changes {
            match value {
                Some(v) => self.set_by_path(path, v.clone())?,
                None => {
                    self.delete_by_path(path)?;
                }
            }
        }
        Ok(())
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
            {"namespace": "module", "identifier": "example-1", "sid": 60000},
            {"namespace": "data", "identifier": "/example-1:greeting", "sid": 60001},
            {"namespace": "data", "identifier": "/example-1:greeting/author", "sid": 60002, "type": "string"},
            {"namespace": "data", "identifier": "/example-1:greeting/message", "sid": 60003, "type": "string"}
        ],
        "key-mapping": {}
    }"#;

    #[test]
    fn test_datastore_set_get() {
        let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
        let mut ds = Datastore::new(model);

        ds.set_by_path("/example-1:greeting/author", Value::String("Obi".into()))
            .unwrap();

        let value = ds.get_by_path("/example-1:greeting/author").unwrap();
        assert_eq!(value, Some(Value::String("Obi".into())));
    }

    #[test]
    fn test_datastore_delete() {
        let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
        let mut ds = Datastore::new(model);

        ds.set_by_path("/example-1:greeting/author", Value::String("Obi".into()))
            .unwrap();
        let deleted = ds.delete_by_path("/example-1:greeting/author").unwrap();

        assert!(deleted);
        assert_eq!(ds.get_by_path("/example-1:greeting/author").unwrap(), None);
    }

    #[test]
    fn test_datastore_from_json() {
        let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
        let json = r#"{"example-1:greeting": {"author": "Obi", "message": "Hello!"}}"#;
        let ds = Datastore::from_json(model, json).unwrap();

        let author = ds.get_by_path("/example-1:greeting/author").unwrap();
        assert_eq!(author, Some(Value::String("Obi".into())));
    }
}
