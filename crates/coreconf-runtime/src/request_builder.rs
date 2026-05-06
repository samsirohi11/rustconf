use coreconf_model::instance_id::{decode_instances, encode_identifiers, encode_instances};
use coreconf_model::{CoreconfModel, Instance, InstancePath, Result};
use serde_json::Value;

#[derive(Debug)]
pub struct RequestBuilder {
    model: CoreconfModel,
}

impl RequestBuilder {
    pub fn new(model: CoreconfModel) -> Self {
        Self { model }
    }

    pub fn build_fetch(&self, paths: &[&str]) -> Result<Vec<u8>> {
        let mut instance_paths = Vec::new();
        for path in paths {
            let instance_path = InstancePath::from_yang_path(path, &self.model.sid_file)?;
            instance_paths.push(instance_path);
        }
        encode_identifiers(&instance_paths)
    }

    pub fn build_fetch_sids(&self, sids: &[i64]) -> Result<Vec<u8>> {
        let mut instance_paths = Vec::new();
        for sid in sids {
            let mut instance_path = InstancePath::new();
            instance_path.push_delta(*sid);
            instance_paths.push(instance_path);
        }
        encode_identifiers(&instance_paths)
    }

    pub fn build_ipatch(&self, changes: &[(&str, Option<Value>)]) -> Result<Vec<u8>> {
        let mut instances = Vec::new();
        for (path, value) in changes {
            let instance_path = InstancePath::from_yang_path(path, &self.model.sid_file)?;
            let instance = match value {
                Some(value) => Instance::new(instance_path, value.clone()),
                None => Instance::delete(instance_path),
            };
            instances.push(instance);
        }
        encode_instances(&instances)
    }

    pub fn build_ipatch_sids(&self, changes: &[(i64, Option<Value>)]) -> Result<Vec<u8>> {
        let mut instances = Vec::new();
        for (sid, value) in changes {
            let mut instance_path = InstancePath::new();
            instance_path.push_delta(*sid);
            let instance = match value {
                Some(value) => Instance::new(instance_path, value.clone()),
                None => Instance::delete(instance_path),
            };
            instances.push(instance);
        }
        encode_instances(&instances)
    }

    pub fn build_post(&self, rpc_path: &str, input: Option<Value>) -> Result<Vec<u8>> {
        let instance_path = InstancePath::from_yang_path(rpc_path, &self.model.sid_file)?;
        let instance = match input {
            Some(value) => Instance::new(instance_path, value),
            None => Instance::new(instance_path, Value::Null),
        };
        encode_instances(&[instance])
    }

    pub fn parse_response(&self, cbor: &[u8]) -> Result<Vec<(i64, Value)>> {
        let instances = decode_instances(cbor)?;
        let mut results = Vec::new();
        for instance in instances {
            if let (Some(sid), Some(value)) = (instance.path.absolute_sid(), instance.value) {
                results.push((sid, value));
            }
        }
        Ok(results)
    }

    pub fn parse_response_json(&self, cbor: &[u8]) -> Result<Value> {
        let mut map = serde_json::Map::new();
        for (sid, value) in self.parse_response(cbor)? {
            if let Some(path) = self.model.sid_file.get_identifier(sid) {
                map.insert(path.to_string(), value);
            }
        }
        Ok(Value::Object(map))
    }
}
