use crate::composite_model::CompositeModel;
use crate::error::{CoreconfError, Result};

fn float_to_decimal64(f: f64) -> Option<(i64, i64)> {
    let s = format!("{:.18}", f);
    let s = s.trim_end_matches('0');
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 2 {
        let exponent = -(parts[1].len() as i64);
        let combined = format!("{}{}", parts[0], parts[1]);
        if let Ok(mantissa) = combined.parse::<i64>() {
            return Some((exponent, mantissa));
        }
    } else if parts.len() == 1 {
        if let Ok(mantissa) = parts[0].parse::<i64>() {
            return Some((0, mantissa));
        }
    }
    None
}

pub fn json_to_cbor_value(
    model: &CompositeModel,
    value: &serde_json::Value,
    parent_sid: i64,
) -> ciborium::value::Value {
    match value {
        serde_json::Value::Null => ciborium::value::Value::Null,
        serde_json::Value::Bool(b) => ciborium::value::Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ciborium::value::Value::Integer(i.into())
            } else if let Some(u) = n.as_u64() {
                ciborium::value::Value::Integer(u.into())
            } else if let Some(f) = n.as_f64() {
                ciborium::value::Value::Float(f)
            } else {
                ciborium::value::Value::Null
            }
        }
        serde_json::Value::String(s) => ciborium::value::Value::Text(s.clone()),
        serde_json::Value::Array(arr) => {
            let vec: Vec<ciborium::value::Value> = arr
                .iter()
                .map(|v| json_to_cbor_value(model, v, parent_sid))
                .collect();
            ciborium::value::Value::Array(vec)
        }
        serde_json::Value::Object(map) => {
            let mut vec = Vec::new();
            for (k, v) in map {
                let key_delta: i64 = k.parse().unwrap_or(0);
                let sid = key_delta + parent_sid;
                
                let key_val = if let Ok(i) = k.parse::<i64>() {
                    ciborium::value::Value::Integer(i.into())
                } else {
                    ciborium::value::Value::Text(k.clone())
                };

                let mut is_binary = false;
                let mut is_decimal64 = false;
                if let Some(identifier) = model.get_identifier(sid) {
                    if let Some(yang_type) = model.get_type(identifier) {
                        if matches!(yang_type, crate::types::YangType::Binary) {
                            is_binary = true;
                        } else if matches!(yang_type, crate::types::YangType::Decimal64) {
                            is_decimal64 = true;
                        }
                    }
                }

                let val_cbor = if is_binary {
                    if let serde_json::Value::Array(arr) = v {
                        let bytes: Vec<u8> = arr
                            .iter()
                            .filter_map(|x| x.as_u64().map(|b| b as u8))
                            .collect();
                        ciborium::value::Value::Bytes(bytes)
                    } else {
                        json_to_cbor_value(model, v, sid)
                    }
                } else if is_decimal64 {
                    if let Some(f) = v.as_f64() {
                        if let Some((exp, mantissa)) = float_to_decimal64(f) {
                            ciborium::value::Value::Tag(
                                4,
                                Box::new(ciborium::value::Value::Array(vec![
                                    ciborium::value::Value::Integer(exp.into()),
                                    ciborium::value::Value::Integer(mantissa.into()),
                                ])),
                            )
                        } else {
                            json_to_cbor_value(model, v, sid)
                        }
                    } else {
                        json_to_cbor_value(model, v, sid)
                    }
                } else {
                    json_to_cbor_value(model, v, sid)
                };

                vec.push((key_val, val_cbor));
            }
            ciborium::value::Value::Map(vec)
        }
    }
}

pub fn encode_json_to_cbor(model: &CompositeModel, json_data: &str) -> Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_str(json_data)?;
    let coreconf_value = model.identifier_value_to_sid_value(value)?;
    let ciborium_val = json_to_cbor_value(model, &coreconf_value, 0);
    let mut bytes = Vec::new();
    ciborium::into_writer(&ciborium_val, &mut bytes)
        .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
    Ok(bytes)
}

pub fn decode_cbor_to_json(model: &CompositeModel, bytes: &[u8]) -> Result<String> {
    let value = cbor_to_json_value(bytes)?;
    let json_value = model.sid_value_to_identifier_value(value)?;
    Ok(serde_json::to_string(&json_value)?)
}

pub fn cbor_to_json_value(bytes: &[u8]) -> Result<serde_json::Value> {
    let mut cursor = std::io::Cursor::new(bytes);
    let ciborium_val: ciborium::value::Value = ciborium::from_reader(&mut cursor)
        .map_err(|e| CoreconfError::CborDecode(e.to_string()))?;
    ciborium_value_to_serde(ciborium_val)
}

pub fn ciborium_value_to_serde(val: ciborium::value::Value) -> Result<serde_json::Value> {
    match val {
        ciborium::value::Value::Null => Ok(serde_json::Value::Null),
        ciborium::value::Value::Bool(b) => Ok(serde_json::Value::Bool(b)),
        ciborium::value::Value::Integer(i) => {
            let num: i128 = i.into();
            if let Ok(n) = i64::try_from(num) {
                Ok(serde_json::Value::Number(n.into()))
            } else if let Ok(n) = u64::try_from(num) {
                Ok(serde_json::Value::Number(n.into()))
            } else {
                Err(CoreconfError::TypeConversion("integer overflow".into()))
            }
        }
        ciborium::value::Value::Float(f) => {
            if let Some(n) = serde_json::Number::from_f64(f) {
                Ok(serde_json::Value::Number(n))
            } else {
                Err(CoreconfError::TypeConversion("invalid float".into()))
            }
        }
        ciborium::value::Value::Text(s) => Ok(serde_json::Value::String(s)),
        ciborium::value::Value::Bytes(b) => {
            let arr = b.into_iter().map(|x| serde_json::Value::Number(x.into())).collect();
            Ok(serde_json::Value::Array(arr))
        }
        ciborium::value::Value::Array(arr) => {
            let mut serde_arr = Vec::with_capacity(arr.len());
            for v in arr {
                serde_arr.push(ciborium_value_to_serde(v)?);
            }
            Ok(serde_json::Value::Array(serde_arr))
        }
        ciborium::value::Value::Map(map) => {
            let mut serde_map = serde_json::Map::new();
            for (k, v) in map {
                let key_str = match k {
                    ciborium::value::Value::Integer(i) => {
                        let val: i128 = i.into();
                        val.to_string()
                    }
                    ciborium::value::Value::Text(s) => s,
                    other => return Err(CoreconfError::TypeConversion(format!("unsupported CBOR map key type: {:?}", other))),
                };
                serde_map.insert(key_str, ciborium_value_to_serde(v)?);
            }
            Ok(serde_json::Value::Object(serde_map))
        }
        ciborium::value::Value::Tag(_, boxed_val) => {
            ciborium_value_to_serde(*boxed_val)
        }
        _ => Err(CoreconfError::TypeConversion("unsupported CBOR type".into())),
    }
}
