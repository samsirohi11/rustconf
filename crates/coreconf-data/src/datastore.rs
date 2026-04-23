//! Unified YANG Datastore management
//!
//! The Datastore stores and manages YANG data instances,
//! supporting get/set/delete operations used by CORECONF handlers.

use crate::coreconf::CoreconfModel;
use crate::error::{CoreconfError, Result};
use crate::instance_id::InstancePath;
use crate::path::{PathExpr, Predicate};
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

    /// Get a value by a path expression with optional list predicates.
    pub fn get_path_expr(&self, input: &str) -> Result<Option<Value>> {
        let expr = PathExpr::parse(input)?;
        let mut current = &self.data;

        for segment in &expr.segments {
            let child = get_object_child(current, &segment.name)?;
            if segment.predicates.is_empty() {
                current = child;
            } else {
                let array = child.as_array().ok_or_else(|| {
                    CoreconfError::ValidationError(format!("expected list node at {}", segment.name))
                })?;
                current = array
                    .iter()
                    .find(|entry| predicates_match(entry, &segment.predicates))
                    .ok_or_else(|| CoreconfError::ResourceNotFound(input.to_string()))?;
            }
        }

        Ok(Some(current.clone()))
    }

    /// Set a value by a path expression with optional list predicates.
    pub fn set_path_expr(&mut self, input: &str, value: Value) -> Result<()> {
        let expr = PathExpr::parse(input)?;
        upsert_segments(&mut self.data, &expr.segments, value)
    }

    /// Delete a value by a path expression with optional list predicates.
    pub fn delete_path_expr(&mut self, input: &str) -> Result<bool> {
        let expr = PathExpr::parse(input)?;
        delete_segments(&mut self.data, &expr.segments)
    }

    /// List predicate selectors for all entries in a YANG list.
    pub fn predicates(&self, input: &str) -> Result<Vec<String>> {
        let expr = PathExpr::parse(input)?;
        let list_path = format!(
            "/{}",
            expr.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>()
                .join("/")
        );
        let node = self
            .model
            .schema
            .get_node(&list_path)
            .ok_or_else(|| CoreconfError::ResourceNotFound(list_path.clone()))?;
        let list_value = self
            .get_path_expr(input)?
            .ok_or_else(|| CoreconfError::ResourceNotFound(input.to_string()))?;
        let array = list_value.as_array().ok_or_else(|| {
            CoreconfError::ValidationError(format!("expected list array at {}", input))
        })?;

        let mut values = array
            .iter()
            .filter_map(|entry| {
                let object = entry.as_object()?;
                let parts: Vec<String> = node
                    .keys
                    .iter()
                    .filter_map(|key_path| {
                        let key_name = key_path.split('/').next_back().unwrap_or(key_path);
                        object.get(key_name).map(|value| {
                            format!("[{}='{}']", key_name, value.as_str().unwrap_or_default())
                        })
                    })
                    .collect();
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join(""))
                }
            })
            .collect::<Vec<_>>();
        values.sort();
        Ok(values)
    }
}

fn get_object_child<'a>(current: &'a Value, key: &str) -> Result<&'a Value> {
    let object = current.as_object().ok_or_else(|| {
        CoreconfError::ValidationError(format!("expected object while resolving {}", key))
    })?;

    object
        .get(key)
        .or_else(|| object.get(key.split(':').next_back().unwrap_or(key)))
        .ok_or_else(|| CoreconfError::ResourceNotFound(key.to_string()))
}

fn predicates_match(entry: &Value, predicates: &[Predicate]) -> bool {
    let object = match entry.as_object() {
        Some(object) => object,
        None => return false,
    };

    predicates.iter().all(|predicate| {
        object
            .get(&predicate.key)
            .and_then(|value| value.as_str())
            .map(|value| value == predicate.value)
            .unwrap_or(false)
    })
}

fn upsert_segments(current: &mut Value, segments: &[crate::path::PathSegment], value: Value) -> Result<()> {
    if segments.is_empty() {
        *current = value;
        return Ok(());
    }

    let segment = &segments[0];
    let is_last = segments.len() == 1;
    let object = current.as_object_mut().ok_or_else(|| {
        CoreconfError::ValidationError(format!("expected object while setting {}", segment.name))
    })?;

    if segment.predicates.is_empty() {
        if is_last {
            object.insert(segment.name.clone(), value);
            return Ok(());
        }

        let child = object
            .entry(segment.name.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        return upsert_segments(child, &segments[1..], value);
    }

    let list_value = object
        .entry(segment.name.clone())
        .or_insert_with(|| Value::Array(Vec::new()));
    let array = list_value.as_array_mut().ok_or_else(|| {
        CoreconfError::ValidationError(format!("expected list at {}", segment.name))
    })?;

    let index = array
        .iter()
        .position(|entry| predicates_match(entry, &segment.predicates))
        .unwrap_or_else(|| {
            let mut entry = Map::new();
            for predicate in &segment.predicates {
                entry.insert(predicate.key.clone(), Value::String(predicate.value.clone()));
            }
            array.push(Value::Object(entry));
            array.len() - 1
        });

    if is_last {
        array[index] = value;
        return Ok(());
    }

    upsert_segments(&mut array[index], &segments[1..], value)
}

fn delete_segments(current: &mut Value, segments: &[crate::path::PathSegment]) -> Result<bool> {
    if segments.is_empty() {
        return Ok(false);
    }

    let segment = &segments[0];
    let is_last = segments.len() == 1;
    let object = current.as_object_mut().ok_or_else(|| {
        CoreconfError::ValidationError(format!("expected object while deleting {}", segment.name))
    })?;

    if segment.predicates.is_empty() {
        if is_last {
            return Ok(object.remove(&segment.name).is_some());
        }

        if let Some(child) = object.get_mut(&segment.name) {
            return delete_segments(child, &segments[1..]);
        }

        return Ok(false);
    }

    let list_value = match object.get_mut(&segment.name) {
        Some(value) => value,
        None => return Ok(false),
    };
    let array = list_value.as_array_mut().ok_or_else(|| {
        CoreconfError::ValidationError(format!("expected list at {}", segment.name))
    })?;

    if let Some(index) = array
        .iter()
        .position(|entry| predicates_match(entry, &segment.predicates))
    {
        if is_last {
            array.remove(index);
            return Ok(true);
        }

        return delete_segments(&mut array[index], &segments[1..]);
    }

    Ok(false)
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
