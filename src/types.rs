//! YANG data type definitions and conversions

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;
use std::collections::HashMap;

use crate::error::{CoreconfError, Result};

type SidLookupFn<'a> = dyn Fn(&str) -> Option<i64> + 'a;

/// Represents YANG data types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YangType {
    String,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Decimal64,
    Binary,
    Boolean,
    Empty,
    Identityref,
    Leafref,
    InstanceIdentifier,
    Bits,
    Uri,
    /// Enumeration with value-to-name mapping
    Enumeration(HashMap<String, i64>),
    /// Union of multiple types
    Union(Vec<YangType>),
    /// Unknown/unrecognized type
    Unknown(String),
}

impl YangType {
    /// Parse a YANG type from SID file type field
    pub fn from_sid_type(type_value: &Value) -> Self {
        match type_value {
            Value::String(s) => Self::from_string(s),
            Value::Object(map) => {
                // Enumeration: {"value": "name", ...}
                let enum_map: HashMap<String, i64> = map
                    .iter()
                    .filter_map(|(k, v)| {
                        v.as_str()
                            .map(|name| (name.to_string(), k.parse().unwrap_or(0)))
                    })
                    .collect();
                YangType::Enumeration(enum_map)
            }
            Value::Array(arr) => {
                // Union of types
                let types: Vec<YangType> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(Self::from_string))
                    .collect();
                YangType::Union(types)
            }
            _ => YangType::Unknown("invalid".to_string()),
        }
    }

    fn from_string(s: &str) -> Self {
        match s {
            "string" => YangType::String,
            "int8" => YangType::Int8,
            "int16" => YangType::Int16,
            "int32" => YangType::Int32,
            "int64" => YangType::Int64,
            "uint8" => YangType::Uint8,
            "uint16" => YangType::Uint16,
            "uint32" => YangType::Uint32,
            "uint64" => YangType::Uint64,
            "decimal64" => YangType::Decimal64,
            "binary" => YangType::Binary,
            "boolean" => YangType::Boolean,
            "empty" => YangType::Empty,
            "identityref" => YangType::Identityref,
            "leafref" => YangType::Leafref,
            "instance-identifier" => YangType::InstanceIdentifier,
            "bits" => YangType::Bits,
            "inet:uri" => YangType::Uri,
            other => YangType::Unknown(other.to_string()),
        }
    }
}

/// Cast a JSON value to CORECONF representation based on YANG type
pub fn cast_to_coreconf(
    value: &Value,
    yang_type: &YangType,
    sid_lookup: Option<&SidLookupFn<'_>>,
) -> Result<Value> {
    match yang_type {
        YangType::String | YangType::Uri => {
            Ok(Value::String(value.as_str().unwrap_or("").to_string()))
        }

        YangType::Int8 | YangType::Int16 | YangType::Int32 | YangType::Int64 => {
            let n = value_to_i64(value)?;
            Ok(Value::Number(n.into()))
        }

        YangType::Uint8 | YangType::Uint16 | YangType::Uint32 | YangType::Uint64 => {
            let n = value_to_u64(value)?;
            Ok(Value::Number(n.into()))
        }

        YangType::Decimal64 => {
            let f = value_to_f64(value)?;
            Ok(serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null))
        }

        YangType::Binary => {
            // Decode base64 string to bytes
            let s = value.as_str().unwrap_or("");
            let bytes = BASE64
                .decode(s)
                .map_err(|e| CoreconfError::TypeConversion(format!("base64 decode: {}", e)))?;
            // Return as array of bytes for CBOR
            Ok(Value::Array(
                bytes.into_iter().map(|b| Value::Number(b.into())).collect(),
            ))
        }

        YangType::Boolean => {
            let b = match value {
                Value::Bool(b) => *b,
                Value::String(s) => s == "true",
                _ => false,
            };
            Ok(Value::Bool(b))
        }

        YangType::Identityref => {
            // Look up SID for "module:identity" format
            if let (Some(s), Some(lookup)) = (value.as_str(), sid_lookup)
                && let Some((_module, identity)) = s.split_once(':')
                    && let Some(sid) = lookup(identity) {
                        return Ok(Value::Number(sid.into()));
                    }
            // Fall back to string
            Ok(value.clone())
        }

        YangType::Enumeration(enum_map) => {
            // Look up enum value by name
            if let Some(s) = value.as_str()
                && let Some(&val) = enum_map.get(s) {
                    return Ok(Value::Number(val.into()));
                }
            // Try numeric lookup
            if let Some(n) = value.as_i64() {
                return Ok(Value::Number(n.into()));
            }
            Err(CoreconfError::TypeConversion(format!(
                "enumeration value not found: {:?}",
                value
            )))
        }

        YangType::Empty | YangType::Leafref | YangType::InstanceIdentifier | YangType::Bits => {
            // Return as-is
            Ok(value.clone())
        }

        YangType::Union(types) => {
            // Try each type in order
            for t in types {
                if let Ok(v) = cast_to_coreconf(value, t, sid_lookup) {
                    return Ok(v);
                }
            }
            Ok(value.clone())
        }

        YangType::Unknown(_) => Ok(value.clone()),
    }
}

/// Cast a CORECONF value back to JSON representation based on YANG type
pub fn cast_from_coreconf(
    value: &Value,
    yang_type: &YangType,
    id_lookup: Option<&dyn Fn(i64) -> Option<String>>,
    module_name: &str,
) -> Result<Value> {
    match yang_type {
        YangType::String | YangType::Uri => {
            Ok(Value::String(value.as_str().unwrap_or("").to_string()))
        }

        YangType::Int8
        | YangType::Int16
        | YangType::Int32
        | YangType::Int64
        | YangType::Uint8
        | YangType::Uint16
        | YangType::Uint32
        | YangType::Uint64 => {
            if let Some(n) = value.as_i64() {
                Ok(Value::Number(n.into()))
            } else if let Some(n) = value.as_u64() {
                Ok(Value::Number(n.into()))
            } else {
                Ok(value.clone())
            }
        }

        YangType::Decimal64 => {
            if let Some(f) = value.as_f64() {
                Ok(serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null))
            } else {
                Ok(value.clone())
            }
        }

        YangType::Binary => {
            // Encode bytes to base64 string
            let bytes: Vec<u8> = match value {
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect(),
                _ => return Ok(value.clone()),
            };
            let encoded = BASE64.encode(&bytes);
            Ok(Value::String(encoded))
        }

        YangType::Boolean => Ok(Value::Bool(value.as_bool().unwrap_or(false))),

        YangType::Identityref => {
            // Look up identifier for SID
            if let (Some(sid), Some(lookup)) = (value.as_i64(), id_lookup)
                && let Some(identifier) = lookup(sid) {
                    let full_ref = format!("{}:{}", module_name, identifier);
                    return Ok(Value::String(full_ref));
                }
            Ok(value.clone())
        }

        YangType::Enumeration(enum_map) => {
            // Look up enum name by value
            if let Some(n) = value.as_i64() {
                for (name, &val) in enum_map {
                    if val == n {
                        return Ok(Value::String(name.clone()));
                    }
                }
            }
            Ok(value.clone())
        }

        YangType::Empty | YangType::Leafref | YangType::InstanceIdentifier | YangType::Bits => {
            Ok(value.clone())
        }

        YangType::Union(types) => {
            for t in types {
                if let Ok(v) = cast_from_coreconf(value, t, id_lookup, module_name) {
                    return Ok(v);
                }
            }
            Ok(value.clone())
        }

        YangType::Unknown(_) => Ok(value.clone()),
    }
}

fn value_to_i64(value: &Value) -> Result<i64> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| CoreconfError::TypeConversion(format!("cannot convert {} to i64", n))),
        Value::String(s) => s
            .parse()
            .map_err(|_| CoreconfError::TypeConversion(format!("cannot parse '{}' as i64", s))),
        _ => Err(CoreconfError::TypeConversion(format!(
            "cannot convert {:?} to i64",
            value
        ))),
    }
}

fn value_to_u64(value: &Value) -> Result<u64> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| CoreconfError::TypeConversion(format!("cannot convert {} to u64", n))),
        Value::String(s) => s
            .parse()
            .map_err(|_| CoreconfError::TypeConversion(format!("cannot parse '{}' as u64", s))),
        _ => Err(CoreconfError::TypeConversion(format!(
            "cannot convert {:?} to u64",
            value
        ))),
    }
}

fn value_to_f64(value: &Value) -> Result<f64> {
    match value {
        Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| CoreconfError::TypeConversion(format!("cannot convert {} to f64", n))),
        Value::String(s) => s
            .parse()
            .map_err(|_| CoreconfError::TypeConversion(format!("cannot parse '{}' as f64", s))),
        _ => Err(CoreconfError::TypeConversion(format!(
            "cannot convert {:?} to f64",
            value
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yang_type_from_string() {
        assert_eq!(YangType::from_string("string"), YangType::String);
        assert_eq!(YangType::from_string("uint8"), YangType::Uint8);
        assert_eq!(YangType::from_string("boolean"), YangType::Boolean);
        assert_eq!(YangType::from_string("inet:uri"), YangType::Uri);
    }

    #[test]
    fn test_cast_string() {
        let value = Value::String("hello".to_string());
        let result = cast_to_coreconf(&value, &YangType::String, None).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_cast_integer() {
        let value = Value::Number(42.into());
        let result = cast_to_coreconf(&value, &YangType::Uint8, None).unwrap();
        assert_eq!(result, Value::Number(42.into()));
    }

    #[test]
    fn test_cast_boolean() {
        let value = Value::String("true".to_string());
        let result = cast_to_coreconf(&value, &YangType::Boolean, None).unwrap();
        assert_eq!(result, Value::Bool(true));
    }
}
