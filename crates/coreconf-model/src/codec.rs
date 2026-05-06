use crate::composite_model::CompositeModel;
use crate::error::{CoreconfError, Result};

pub fn encode_json_to_cbor(model: &CompositeModel, json_data: &str) -> Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_str(json_data)?;
    let coreconf_value = model.identifier_value_to_sid_value(value)?;
    let mut bytes = Vec::new();
    ciborium::into_writer(&coreconf_value, &mut bytes)
        .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
    Ok(bytes)
}

pub fn decode_cbor_to_json(model: &CompositeModel, bytes: &[u8]) -> Result<String> {
    let value: serde_json::Value =
        ciborium::from_reader(bytes).map_err(|e| CoreconfError::CborDecode(e.to_string()))?;
    let json_value = model.sid_value_to_identifier_value(value)?;
    Ok(serde_json::to_string(&json_value)?)
}
