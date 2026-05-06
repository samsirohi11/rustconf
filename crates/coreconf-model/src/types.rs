use std::collections::HashMap;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;

use crate::error::{CoreconfError, Result};

type SidLookupFn<'a> = dyn Fn(&str) -> Option<i64> + 'a;

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
    Enumeration(HashMap<String, i64>),
    Union(Vec<YangType>),
    Unknown(String),
}

impl YangType {
    pub fn from_sid_type(type_value: &Value) -> Result<Self> {
        match type_value {
            Value::String(s) => Ok(Self::from_string(s)),
            Value::Object(map) => {
                let mut enum_map = HashMap::with_capacity(map.len());
                for (raw_value, name) in map {
                    let name = name.as_str().ok_or_else(|| {
                        CoreconfError::InvalidSidFile(format!(
                            "enumeration entry '{raw_value}' must map to a string name"
                        ))
                    })?;
                    let numeric_value = raw_value.parse().map_err(|_| {
                        CoreconfError::InvalidSidFile(format!(
                            "enumeration value '{raw_value}' is not a valid i64"
                        ))
                    })?;
                    enum_map.insert(name.to_string(), numeric_value);
                }
                Ok(YangType::Enumeration(enum_map))
            }
            Value::Array(arr) => {
                let mut types = Vec::with_capacity(arr.len());
                for entry in arr {
                    let type_name = entry.as_str().ok_or_else(|| {
                        CoreconfError::InvalidSidFile(format!(
                            "union member must be a string, got {entry:?}"
                        ))
                    })?;
                    types.push(Self::strict_from_string(type_name)?);
                }
                Ok(YangType::Union(types))
            }
            _ => Err(CoreconfError::InvalidSidFile(format!(
                "unsupported SID type metadata: {type_value:?}"
            ))),
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

    fn strict_from_string(s: &str) -> Result<Self> {
        let yang_type = Self::from_string(s);
        if matches!(yang_type, YangType::Unknown(_)) {
            return Err(CoreconfError::InvalidSidFile(format!(
                "unknown YANG type '{s}'"
            )));
        }
        Ok(yang_type)
    }
}

pub fn cast_to_coreconf(
    value: &Value,
    yang_type: &YangType,
    sid_lookup: Option<&SidLookupFn<'_>>,
) -> Result<Value> {
    match yang_type {
        YangType::String | YangType::Uri => {
            let s = value.as_str().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to string-compatible value"
                ))
            })?;
            Ok(Value::String(s.to_string()))
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
            serde_json::Number::from_f64(f)
                .map(Value::Number)
                .ok_or_else(|| {
                    CoreconfError::TypeConversion(format!(
                        "cannot represent {f} as decimal64 JSON number"
                    ))
                })
        }
        YangType::Binary => {
            let s = value.as_str().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to base64-encoded binary string"
                ))
            })?;
            let bytes = BASE64
                .decode(s)
                .map_err(|e| CoreconfError::TypeConversion(format!("base64 decode: {e}")))?;
            Ok(Value::Array(
                bytes.into_iter().map(|b| Value::Number(b.into())).collect(),
            ))
        }
        YangType::Boolean => {
            let b = match value {
                Value::Bool(b) => *b,
                Value::String(s) => match s.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(CoreconfError::TypeConversion(format!(
                            "cannot parse '{s}' as boolean"
                        )));
                    }
                },
                _ => {
                    return Err(CoreconfError::TypeConversion(format!(
                        "cannot convert {value:?} to boolean"
                    )));
                }
            };
            Ok(Value::Bool(b))
        }
        YangType::Identityref => {
            let s = value.as_str().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to identityref"
                ))
            })?;
            let lookup = sid_lookup.ok_or_else(|| {
                CoreconfError::TypeConversion(
                    "cannot convert identityref without SID lookup".to_string(),
                )
            })?;
            let sid = lookup(s)
                .or_else(|| s.split_once(':').and_then(|(_, identity)| lookup(identity)))
                .ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "identityref value not found: {s}"
                ))
            })?;
            Ok(Value::Number(sid.into()))
        }
        YangType::Enumeration(enum_map) => {
            if let Some(s) = value.as_str()
                && let Some(&val) = enum_map.get(s)
            {
                return Ok(Value::Number(val.into()));
            }
            if let Some(n) = value.as_i64() {
                if enum_map.values().any(|&val| val == n) {
                    return Ok(Value::Number(n.into()));
                }
            }
            Err(CoreconfError::TypeConversion(format!(
                "enumeration value not found: {value:?}"
            )))
        }
        YangType::Empty | YangType::Leafref | YangType::InstanceIdentifier | YangType::Bits => {
            Ok(value.clone())
        }
        YangType::Union(types) => {
            for t in types {
                if let Ok(v) = cast_to_coreconf(value, t, sid_lookup) {
                    return Ok(v);
                }
            }
            Err(CoreconfError::TypeConversion(format!(
                "value {value:?} does not match any union member"
            )))
        }
        YangType::Unknown(_) => Ok(value.clone()),
    }
}

pub fn cast_from_coreconf(
    value: &Value,
    yang_type: &YangType,
    id_lookup: Option<&dyn Fn(i64) -> Option<String>>,
    module_name: &str,
) -> Result<Value> {
    match yang_type {
        YangType::String | YangType::Uri => {
            let s = value.as_str().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to string-compatible value"
                ))
            })?;
            Ok(Value::String(s.to_string()))
        }
        YangType::Int8
        | YangType::Int16
        | YangType::Int32
        | YangType::Int64
        | YangType::Uint8
        | YangType::Uint16
        | YangType::Uint32
        | YangType::Uint64 => {
            match yang_type {
                YangType::Int8 | YangType::Int16 | YangType::Int32 | YangType::Int64 => {
                    Ok(Value::Number(value_to_i64(value)?.into()))
                }
                YangType::Uint8 | YangType::Uint16 | YangType::Uint32 | YangType::Uint64 => {
                    Ok(Value::Number(value_to_u64(value)?.into()))
                }
                _ => unreachable!(),
            }
        }
        YangType::Decimal64 => {
            let f = value_to_f64(value)?;
            serde_json::Number::from_f64(f)
                .map(Value::Number)
                .ok_or_else(|| {
                    CoreconfError::TypeConversion(format!(
                        "cannot represent {f} as decimal64 JSON number"
                    ))
                })
        }
        YangType::Binary => {
            let arr = value.as_array().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to binary byte array"
                ))
            })?;
            let mut bytes = Vec::with_capacity(arr.len());
            for entry in arr {
                let byte = entry.as_u64().ok_or_else(|| {
                    CoreconfError::TypeConversion(format!(
                        "binary value contains non-byte entry: {entry:?}"
                    ))
                })?;
                let byte = u8::try_from(byte).map_err(|_| {
                    CoreconfError::TypeConversion(format!(
                        "binary value contains out-of-range byte: {byte}"
                    ))
                })?;
                bytes.push(byte);
            }
            Ok(Value::String(BASE64.encode(&bytes)))
        }
        YangType::Boolean => {
            let b = value.as_bool().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to boolean"
                ))
            })?;
            Ok(Value::Bool(b))
        }
        YangType::Identityref => {
            let sid = value.as_i64().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to identityref"
                ))
            })?;
            let lookup = id_lookup.ok_or_else(|| {
                CoreconfError::TypeConversion(
                    "cannot convert identityref without identifier lookup".to_string(),
                )
            })?;
            let identifier = lookup(sid).ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "identityref SID not found: {sid}"
                ))
            })?;
            Ok(Value::String(format_identityref(&identifier, module_name)))
        }
        YangType::Enumeration(enum_map) => {
            let n = value.as_i64().ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "cannot convert {value:?} to enumeration value"
                ))
            })?;
            for (name, &val) in enum_map {
                if val == n {
                    return Ok(Value::String(name.clone()));
                }
            }
            Err(CoreconfError::TypeConversion(format!(
                "enumeration value not found for numeric value {n}"
            )))
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
            Err(CoreconfError::TypeConversion(format!(
                "value {value:?} does not match any union member"
            )))
        }
        YangType::Unknown(_) => Ok(value.clone()),
    }
}

fn value_to_i64(value: &Value) -> Result<i64> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| CoreconfError::TypeConversion(format!("cannot convert {n} to i64"))),
        Value::String(s) => s
            .parse()
            .map_err(|_| CoreconfError::TypeConversion(format!("cannot parse '{s}' as i64"))),
        _ => Err(CoreconfError::TypeConversion(format!(
            "cannot convert {value:?} to i64"
        ))),
    }
}

fn value_to_u64(value: &Value) -> Result<u64> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| CoreconfError::TypeConversion(format!("cannot convert {n} to u64"))),
        Value::String(s) => s
            .parse()
            .map_err(|_| CoreconfError::TypeConversion(format!("cannot parse '{s}' as u64"))),
        _ => Err(CoreconfError::TypeConversion(format!(
            "cannot convert {value:?} to u64"
        ))),
    }
}

fn value_to_f64(value: &Value) -> Result<f64> {
    match value {
        Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| CoreconfError::TypeConversion(format!("cannot convert {n} to f64"))),
        Value::String(s) => s
            .parse()
            .map_err(|_| CoreconfError::TypeConversion(format!("cannot parse '{s}' as f64"))),
        _ => Err(CoreconfError::TypeConversion(format!(
            "cannot convert {value:?} to f64"
        ))),
    }
}

fn format_identityref(identifier: &str, module_name: &str) -> String {
    let normalized = identifier.trim_start_matches('/');
    if normalized.contains(':') || module_name.is_empty() {
        normalized.to_string()
    } else {
        format!("{module_name}:{normalized}")
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

    #[test]
    fn test_cast_string_rejects_non_strings() {
        let value = Value::Number(42.into());
        let err = cast_to_coreconf(&value, &YangType::String, None).unwrap_err();
        assert!(matches!(err, CoreconfError::TypeConversion(message) if message.contains("string")));
    }

    #[test]
    fn test_cast_boolean_rejects_invalid_strings() {
        let value = Value::String("yes".to_string());
        let err = cast_to_coreconf(&value, &YangType::Boolean, None).unwrap_err();
        assert!(matches!(err, CoreconfError::TypeConversion(message) if message.contains("boolean")));
    }

    #[test]
    fn test_cast_binary_from_coreconf_rejects_invalid_bytes() {
        let value = Value::Array(vec![Value::Number(255.into()), Value::Number(256.into())]);
        let err = cast_from_coreconf(&value, &YangType::Binary, None, "example").unwrap_err();
        assert!(matches!(err, CoreconfError::TypeConversion(message) if message.contains("binary")));
    }

    #[test]
    fn test_cast_identityref_rejects_unknown_identity() {
        let value = Value::String("example:missing".to_string());
        let lookup = |_identifier: &str| None;
        let err = cast_to_coreconf(&value, &YangType::Identityref, Some(&lookup)).unwrap_err();
        assert!(matches!(err, CoreconfError::TypeConversion(message) if message.contains("identityref")));
    }

    #[test]
    fn test_cast_identityref_to_coreconf_accepts_qualified_names() {
        let value = Value::String("example:up".to_string());
        let lookup = |identifier: &str| (identifier == "example:up").then_some(42);
        let converted = cast_to_coreconf(&value, &YangType::Identityref, Some(&lookup)).unwrap();
        assert_eq!(converted, Value::Number(42.into()));
    }

    #[test]
    fn test_cast_identityref_from_coreconf_preserves_qualified_names() {
        let value = Value::Number(42.into());
        let lookup = |_sid: i64| Some("example:up".to_string());
        let converted =
            cast_from_coreconf(&value, &YangType::Identityref, Some(&lookup), "example").unwrap();
        assert_eq!(converted, Value::String("example:up".to_string()));
    }

    #[test]
    fn test_cast_enumeration_rejects_unknown_numeric_values() {
        let value = Value::Number(99.into());
        let yang_type = YangType::Enumeration(HashMap::from([("up".to_string(), 1)]));
        let err = cast_to_coreconf(&value, &yang_type, None).unwrap_err();
        assert!(matches!(err, CoreconfError::TypeConversion(message) if message.contains("enumeration")));
    }
}
