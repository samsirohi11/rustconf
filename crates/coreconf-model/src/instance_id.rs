use serde_json::Value;

use crate::composite_model::CompositeModel;
use crate::error::{CoreconfError, Result};
use crate::sid_file::SidFile;

#[derive(Debug, Clone, PartialEq)]
pub enum PathComponent {
    SidDelta(i64),
    KeyValue(Value),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InstancePath {
    pub components: Vec<PathComponent>,
    absolute_sid: Option<i64>,
}

impl InstancePath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_yang_path(path: &str, sid_file: &SidFile) -> Result<Self> {
        let mut components = Vec::new();
        let mut current_sid = 0i64;
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for (i, _) in parts.iter().enumerate() {
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

    pub fn push_delta(&mut self, delta: i64) {
        self.components.push(PathComponent::SidDelta(delta));
        if let Some(ref mut sid) = self.absolute_sid {
            *sid += delta;
        } else {
            self.absolute_sid = Some(delta);
        }
    }

    pub fn push_key(&mut self, key: Value) {
        self.components.push(PathComponent::KeyValue(key));
    }

    pub fn absolute_sid(&self) -> Option<i64> {
        self.absolute_sid
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn encode_cbor(&self) -> Result<Vec<u8>> {
        let value = self.to_cbor_value();
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes)
            .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
        Ok(bytes)
    }

    pub fn to_cbor_value(&self) -> Value {
        if self.components.is_empty() {
            return Value::Null;
        }

        if self.components.len() == 1
            && let PathComponent::SidDelta(delta) = &self.components[0]
        {
            return Value::Number((*delta).into());
        }

        Value::Array(
            self.components
                .iter()
                .map(|component| match component {
                    PathComponent::SidDelta(delta) => Value::Number((*delta).into()),
                    PathComponent::KeyValue(value) => value.clone(),
                })
                .collect(),
        )
    }

    pub fn decode_cbor(bytes: &[u8]) -> Result<Self> {
        let value: Value =
            ciborium::from_reader(bytes).map_err(|e| CoreconfError::CborDecode(e.to_string()))?;
        Self::from_cbor_value(&value)
    }

    pub fn from_cbor_value(value: &Value) -> Result<Self> {
        let mut path = Self::new();

        match value {
            Value::Null => {}
            Value::Number(n) => {
                let delta = n
                    .as_i64()
                    .ok_or_else(|| CoreconfError::TypeConversion("expected integer SID".into()))?;
                path.push_delta(delta);
            }
            Value::Array(arr) => {
                let mut index = 0;
                while index < arr.len() {
                    let delta = arr[index].as_i64().ok_or_else(|| {
                        CoreconfError::TypeConversion("expected SID delta".into())
                    })?;
                    path.push_delta(delta);
                    index += 1;

                    while index < arr.len() && arr[index].as_i64().is_none() {
                        path.push_key(arr[index].clone());
                        index += 1;
                    }
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

    fn from_cbor_value_with_model(value: &Value, model: &CompositeModel) -> Result<Self> {
        let Value::Array(values) = value else {
            return Self::from_cbor_value(value);
        };
        if values.is_empty() {
            return Err(CoreconfError::TypeConversion(
                "instance identifier array cannot be empty".into(),
            ));
        }

        let mut path = Self::new();
        let mut index = 0usize;
        while index < values.len() {
            let delta = values[index].as_i64().ok_or_else(|| {
                CoreconfError::TypeConversion("expected SID delta in instance identifier".into())
            })?;
            path.push_delta(delta);
            let sid = path.absolute_sid().ok_or_else(|| {
                CoreconfError::TypeConversion("instance identifier has no SID".into())
            })?;
            if model.get_identifier(sid).is_none() {
                return Err(CoreconfError::SidNotFound(sid.to_string()));
            }
            index += 1;

            let key_count = model.get_keys(sid).map_or(0, Vec::len);
            for _ in 0..key_count {
                let key = values.get(index).ok_or_else(|| {
                    CoreconfError::TypeConversion(format!(
                        "instance identifier is missing a key for list SID {sid}"
                    ))
                })?;
                if !matches!(key, Value::Bool(_) | Value::Number(_) | Value::String(_)) {
                    return Err(CoreconfError::TypeConversion(
                        "unsupported list key value in instance identifier".into(),
                    ));
                }
                path.push_key(key.clone());
                index += 1;
            }
        }
        Ok(path)
    }
}

pub fn encode_identifiers(paths: &[InstancePath]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for path in paths {
        let value = path.to_cbor_value();
        ciborium::into_writer(&value, &mut bytes)
            .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub path: InstancePath,
    pub value: Option<Value>,
}

impl Instance {
    pub fn new(path: InstancePath, value: Value) -> Self {
        Self {
            path,
            value: Some(value),
        }
    }

    pub fn delete(path: InstancePath) -> Self {
        Self { path, value: None }
    }

    pub fn to_cbor_value(&self) -> Value {
        let sid = self.path.absolute_sid().unwrap_or(0);
        let value = self.value.clone().unwrap_or(Value::Null);
        let mut map = serde_json::Map::new();
        map.insert(sid.to_string(), value);
        Value::Object(map)
    }
}

pub fn encode_instances(instances: &[Instance]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for instance in instances {
        let value = instance.to_cbor_value();
        ciborium::into_writer(&value, &mut bytes)
            .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
    }
    Ok(bytes)
}

pub fn decode_instances(bytes: &[u8]) -> Result<Vec<Instance>> {
    decode_instances_with_path_decoder(bytes, InstancePath::from_cbor_value)
}

/// Decodes an instance sequence using SID key-mapping metadata.
///
/// The ordinary instance-identifier representation does not mark integer list
/// keys. This decoder consumes exactly the number of keys declared for each
/// keyed-list SID, so integer keys cannot be mistaken for subsequent SID
/// deltas. Use [`decode_instances`] when the caller intentionally needs the
/// legacy, metadata-free representation.
///
/// # Errors
///
/// Returns an error for malformed CBOR, unknown SIDs, missing list keys, or
/// unsupported key values.
pub fn decode_instances_with_model(model: &CompositeModel, bytes: &[u8]) -> Result<Vec<Instance>> {
    decode_instances_with_path_decoder(bytes, |value| {
        InstancePath::from_cbor_value_with_model(value, model)
    })
}

fn decode_instances_with_path_decoder(
    bytes: &[u8],
    mut decode_path: impl FnMut(&Value) -> Result<InstancePath>,
) -> Result<Vec<Instance>> {
    let mut instances = Vec::new();
    let mut cursor = std::io::Cursor::new(bytes);

    while (cursor.position() as usize) < bytes.len() {
        let ciborium_val: ciborium::value::Value = ciborium::from_reader(&mut cursor)
            .map_err(|e| CoreconfError::CborDecode(e.to_string()))?;

        let ciborium::value::Value::Map(entries) = ciborium_val else {
            return Err(CoreconfError::TypeConversion(
                "invalid instance payload: expected map".into(),
            ));
        };
        for (key, value) in entries {
            let path = match key {
                ciborium::value::Value::Integer(integer) => {
                    let sid = i64::try_from(integer).map_err(|_| {
                        CoreconfError::TypeConversion("invalid SID in instance".into())
                    })?;
                    let mut path = InstancePath::new();
                    path.push_delta(sid);
                    path
                }
                ciborium::value::Value::Text(text) => {
                    let sid = text.parse::<i64>().map_err(|_| {
                        CoreconfError::TypeConversion("invalid SID in instance".into())
                    })?;
                    let mut path = InstancePath::new();
                    path.push_delta(sid);
                    path
                }
                other => {
                    let key = crate::codec::ciborium_value_to_serde(other)?;
                    decode_path(&key)?
                }
            };
            let value = crate::codec::ciborium_value_to_serde(value)?;
            if value.is_null() {
                instances.push(Instance::delete(path));
            } else {
                instances.push(Instance::new(path, value));
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

        let cbor = path.encode_cbor().expect("encode instance path");
        let decoded = InstancePath::decode_cbor(&cbor).expect("decode instance path");

        assert_eq!(decoded.absolute_sid(), Some(60001));
    }

    #[test]
    fn test_instance_path_with_key() {
        let mut path = InstancePath::new();
        path.push_delta(1756);
        path.push_key(Value::String("myserver".into()));

        assert!(path.to_cbor_value().is_array());
    }

    #[test]
    fn test_instance_path_decodes_multi_key_segment() {
        let decoded = InstancePath::from_cbor_value(&Value::Array(vec![
            Value::Number(1756.into()),
            Value::String("tenant-a".into()),
            Value::String("interface-1".into()),
            Value::Number(2.into()),
        ]))
        .expect("decode multi-key path");

        assert_eq!(
            decoded.components,
            vec![
                PathComponent::SidDelta(1756),
                PathComponent::KeyValue(Value::String("tenant-a".into())),
                PathComponent::KeyValue(Value::String("interface-1".into())),
                PathComponent::SidDelta(2),
            ]
        );
        assert_eq!(decoded.absolute_sid(), Some(1758));
    }

    #[test]
    fn test_model_aware_path_round_trips_integer_list_keys() {
        let model = CompositeModel::from_sid_strings(&[r#"{
            "module-name":"example",
            "module-revision":"2026-01-01",
            "item":[
                {"identifier":"example","sid":2574},
                {"identifier":"/example:rule","sid":2597},
                {"identifier":"/example:rule/value","sid":2599},
                {"identifier":"/example:rule/length","sid":2598},
                {"identifier":"/example:rule/leaf","sid":2600}
            ],
            "key-mapping":{"2597":[2599,2598]}
        }"#])
        .expect("model");
        let mut path = InstancePath::new();
        path.push_delta(2574);
        path.push_delta(23);
        path.push_key(Value::Number(20.into()));
        path.push_key(Value::Number(8.into()));
        path.push_delta(3);
        let decoded = InstancePath::from_cbor_value_with_model(&path.to_cbor_value(), &model)
            .expect("integer key path");
        assert_eq!(decoded, path);
    }

    #[test]
    fn test_encode_instances() {
        let mut path = InstancePath::new();
        path.push_delta(1755);
        let instance = Instance::new(path, Value::Bool(true));

        let bytes = encode_instances(&[instance]).expect("encode instances");
        let decoded = decode_instances(&bytes).expect("decode instances");

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].value, Some(Value::Bool(true)));
    }
}
