//! RFC 9595 Instance Identifier encoding/decoding
//!
//! Instance identifiers are used in CORECONF to identify specific data nodes
//! in requests and responses. They are encoded as CBOR arrays of SID deltas
//! and key values.

use crate::error::{CoreconfError, Result};
use crate::sid::SidFile;
use serde_json::Value;

/// Represents a path component in an instance identifier
#[derive(Debug, Clone, PartialEq)]
pub enum PathComponent {
    /// A SID delta to a child node
    SidDelta(i64),
    /// A key value for list entry selection
    KeyValue(Value),
}

/// Represents a complete instance identifier path
#[derive(Debug, Clone, Default)]
pub struct InstancePath {
    /// The components of this path
    pub components: Vec<PathComponent>,
    /// The absolute SID (computed from deltas)
    absolute_sid: Option<i64>,
}

impl InstancePath {
    /// Create an empty instance path
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from a YANG path string like "/example:container/leaf"
    pub fn from_yang_path(path: &str, sid_file: &SidFile) -> Result<Self> {
        let mut components = Vec::new();
        let mut current_sid = 0i64;

        // Split path and resolve each component
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for (i, _part) in parts.iter().enumerate() {
            // Build the full path up to this component
            let full_path = format!("/{}", parts[..=i].join("/"));

            if let Some(sid) = sid_file.get_sid(&full_path) {
                let delta = sid - current_sid;
                components.push(PathComponent::SidDelta(delta));
                current_sid = sid;
            } else {
                return Err(CoreconfError::SidNotFound(full_path));
            }
        }

        Ok(Self {
            components,
            absolute_sid: Some(current_sid),
        })
    }

    /// Add a SID delta component
    pub fn push_delta(&mut self, delta: i64) {
        self.components.push(PathComponent::SidDelta(delta));
        // Update absolute SID
        if let Some(ref mut sid) = self.absolute_sid {
            *sid += delta;
        } else {
            self.absolute_sid = Some(delta);
        }
    }

    /// Add a key value component
    pub fn push_key(&mut self, key: Value) {
        self.components.push(PathComponent::KeyValue(key));
    }

    /// Get the absolute SID this path points to
    pub fn absolute_sid(&self) -> Option<i64> {
        self.absolute_sid
    }

    /// Check if this path is empty
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Get the number of components
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Encode to CBOR according to RFC 9595
    /// Format: integer (single SID) or array of alternating SID deltas and keys
    pub fn encode_cbor(&self) -> Result<Vec<u8>> {
        let value = self.to_cbor_value();
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes)
            .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
        Ok(bytes)
    }

    /// Convert to a CBOR value
    pub fn to_cbor_value(&self) -> Value {
        if self.components.is_empty() {
            return Value::Null;
        }

        // Simple case: single SID delta with no keys
        if self.components.len() == 1
            && let PathComponent::SidDelta(delta) = &self.components[0] {
                return Value::Number((*delta).into());
            }

        // Complex case: array of deltas and keys
        let arr: Vec<Value> = self
            .components
            .iter()
            .map(|c| match c {
                PathComponent::SidDelta(delta) => Value::Number((*delta).into()),
                PathComponent::KeyValue(v) => v.clone(),
            })
            .collect();

        Value::Array(arr)
    }

    /// Decode from CBOR bytes
    pub fn decode_cbor(bytes: &[u8]) -> Result<Self> {
        let value: Value =
            ciborium::from_reader(bytes).map_err(|e| CoreconfError::CborDecode(e.to_string()))?;
        Self::from_cbor_value(&value)
    }

    /// Decode from a CBOR value
    pub fn from_cbor_value(value: &Value) -> Result<Self> {
        let mut path = Self::new();

        match value {
            Value::Null => {
                // Empty path
            }
            Value::Number(n) => {
                // Single SID
                let delta = n
                    .as_i64()
                    .ok_or_else(|| CoreconfError::TypeConversion("expected integer SID".into()))?;
                path.push_delta(delta);
            }
            Value::Array(arr) => {
                // Alternating SID deltas and keys
                let mut expect_delta = true;
                for item in arr {
                    if expect_delta {
                        if let Some(n) = item.as_i64() {
                            path.push_delta(n);
                        } else {
                            return Err(CoreconfError::TypeConversion("expected SID delta".into()));
                        }
                    } else {
                        path.push_key(item.clone());
                    }
                    expect_delta = !expect_delta;
                }
            }
            _ => {
                return Err(CoreconfError::TypeConversion(
                    "invalid instance identifier format".into(),
                ));
            }
        }

        Ok(path)
    }
}

/// Encode multiple instance identifiers as a CBOR sequence
/// Used for FETCH requests (application/yang-identifiers+cbor)
pub fn encode_identifiers(paths: &[InstancePath]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for path in paths {
        let value = path.to_cbor_value();
        ciborium::into_writer(&value, &mut bytes)
            .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
    }
    Ok(bytes)
}

/// Instance with its value for iPATCH/response
#[derive(Debug, Clone)]
pub struct Instance {
    /// The instance identifier (path)
    pub path: InstancePath,
    /// The value (None = delete for iPATCH)
    pub value: Option<Value>,
}

impl Instance {
    /// Create a new instance
    pub fn new(path: InstancePath, value: Value) -> Self {
        Self {
            path,
            value: Some(value),
        }
    }

    /// Create a delete instance (null value)
    pub fn delete(path: InstancePath) -> Self {
        Self { path, value: None }
    }

    /// Encode as a CBOR map {sid: value}
    pub fn to_cbor_value(&self) -> Value {
        let sid = self.path.absolute_sid().unwrap_or(0);
        let value = self.value.clone().unwrap_or(Value::Null);

        let mut map = serde_json::Map::new();
        map.insert(sid.to_string(), value);
        Value::Object(map)
    }
}

/// Encode multiple instances as CBOR-seq
/// Used for iPATCH requests and responses (application/yang-instances+cbor-seq)
pub fn encode_instances(instances: &[Instance]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for instance in instances {
        let value = instance.to_cbor_value();
        ciborium::into_writer(&value, &mut bytes)
            .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
    }
    Ok(bytes)
}

/// Decode instances from CBOR-seq bytes
pub fn decode_instances(bytes: &[u8]) -> Result<Vec<Instance>> {
    let mut instances = Vec::new();
    let mut cursor = std::io::Cursor::new(bytes);

    while (cursor.position() as usize) < bytes.len() {
        let value: Value = ciborium::from_reader(&mut cursor)
            .map_err(|e| CoreconfError::CborDecode(e.to_string()))?;

        if let Value::Object(map) = value {
            for (key, val) in map {
                let sid: i64 = key
                    .parse()
                    .map_err(|_| CoreconfError::TypeConversion("invalid SID in instance".into()))?;

                let mut path = InstancePath::new();
                path.push_delta(sid);

                let instance = if val.is_null() {
                    Instance::delete(path)
                } else {
                    Instance::new(path, val)
                };
                instances.push(instance);
            }
        }
    }

    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_path_single_delta() {
        let mut path = InstancePath::new();
        path.push_delta(60001);

        let cbor = path.encode_cbor().unwrap();
        let decoded = InstancePath::decode_cbor(&cbor).unwrap();

        assert_eq!(decoded.absolute_sid(), Some(60001));
    }

    #[test]
    fn test_instance_path_with_key() {
        let mut path = InstancePath::new();
        path.push_delta(1756);
        path.push_key(Value::String("myserver".into()));

        let value = path.to_cbor_value();
        assert!(value.is_array());
    }

    #[test]
    fn test_encode_instances() {
        let mut path = InstancePath::new();
        path.push_delta(1755);
        let instance = Instance::new(path, Value::Bool(true));

        let bytes = encode_instances(&[instance]).unwrap();
        let decoded = decode_instances(&bytes).unwrap();

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].value, Some(Value::Bool(true)));
    }
}
