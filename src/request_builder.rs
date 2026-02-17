//! Client-side request building utilities
//!
//! Helpers for constructing CORECONF request payloads and parsing responses.

use crate::coreconf::CoreconfModel;
use crate::error::Result;
use crate::instance_id::{Instance, InstancePath, encode_identifiers, encode_instances};
use serde_json::Value;

/// Client-side request builder for CORECONF operations
#[derive(Debug)]
pub struct RequestBuilder {
    /// The CORECONF model for SID lookups
    model: CoreconfModel,
}

impl RequestBuilder {
    /// Create a new request builder
    pub fn new(model: CoreconfModel) -> Self {
        Self { model }
    }

    /// Build FETCH request payload for given YANG paths
    ///
    /// # Arguments
    /// * `paths` - YANG paths like "/example:container/leaf"
    ///
    /// # Returns
    /// CBOR-encoded payload (application/yang-identifiers+cbor)
    pub fn build_fetch(&self, paths: &[&str]) -> Result<Vec<u8>> {
        let mut instance_paths = Vec::new();

        for path in paths {
            let ip = InstancePath::from_yang_path(path, &self.model.sid_file)?;
            instance_paths.push(ip);
        }

        encode_identifiers(&instance_paths)
    }

    /// Build FETCH request payload for given SIDs
    pub fn build_fetch_sids(&self, sids: &[i64]) -> Result<Vec<u8>> {
        let mut instance_paths = Vec::new();

        for &sid in sids {
            let mut ip = InstancePath::new();
            ip.push_delta(sid);
            instance_paths.push(ip);
        }

        encode_identifiers(&instance_paths)
    }

    /// Build iPATCH request payload
    ///
    /// # Arguments
    /// * `changes` - List of (path, value) pairs. None value means delete.
    ///
    /// # Returns
    /// CBOR-encoded payload (application/yang-instances+cbor-seq)
    pub fn build_ipatch(&self, changes: &[(&str, Option<Value>)]) -> Result<Vec<u8>> {
        let mut instances = Vec::new();

        for (path, value) in changes {
            let ip = InstancePath::from_yang_path(path, &self.model.sid_file)?;

            let instance = match value {
                Some(v) => Instance::new(ip, v.clone()),
                None => Instance::delete(ip),
            };
            instances.push(instance);
        }

        encode_instances(&instances)
    }

    /// Build iPATCH request payload using SIDs
    pub fn build_ipatch_sids(&self, changes: &[(i64, Option<Value>)]) -> Result<Vec<u8>> {
        let mut instances = Vec::new();

        for (sid, value) in changes {
            let mut ip = InstancePath::new();
            ip.push_delta(*sid);

            let instance = match value {
                Some(v) => Instance::new(ip, v.clone()),
                None => Instance::delete(ip),
            };
            instances.push(instance);
        }

        encode_instances(&instances)
    }

    /// Build POST (RPC) request payload
    ///
    /// # Arguments
    /// * `rpc_path` - Path to the RPC like "/example:reboot"
    /// * `input` - Optional input parameters
    pub fn build_post(&self, rpc_path: &str, input: Option<Value>) -> Result<Vec<u8>> {
        let ip = InstancePath::from_yang_path(rpc_path, &self.model.sid_file)?;
        let instance = match input {
            Some(v) => Instance::new(ip, v),
            None => Instance::new(ip, Value::Null),
        };
        encode_instances(&[instance])
    }

    /// Parse a FETCH/iPATCH response
    ///
    /// # Returns
    /// Map of SID -> Value
    pub fn parse_response(&self, cbor: &[u8]) -> Result<Vec<(i64, Value)>> {
        let instances = crate::instance_id::decode_instances(cbor)?;

        let mut results = Vec::new();
        for instance in instances {
            if let (Some(sid), Some(value)) = (instance.path.absolute_sid(), instance.value) {
                results.push((sid, value));
            }
        }

        Ok(results)
    }

    /// Parse response and convert to JSON with YANG paths
    pub fn parse_response_json(&self, cbor: &[u8]) -> Result<Value> {
        let instances = self.parse_response(cbor)?;

        let mut map = serde_json::Map::new();
        for (sid, value) in instances {
            if let Some(path) = self.model.sid_file.get_identifier(sid) {
                map.insert(path.to_string(), value);
            }
        }

        Ok(Value::Object(map))
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
    fn test_build_fetch() {
        let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
        let builder = RequestBuilder::new(model);

        let payload = builder.build_fetch(&["/example-1:greeting"]).unwrap();
        assert!(!payload.is_empty());
    }

    #[test]
    fn test_build_ipatch() {
        let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
        let builder = RequestBuilder::new(model);

        let payload = builder
            .build_ipatch(&[(
                "/example-1:greeting/author",
                Some(Value::String("Luke".into())),
            )])
            .unwrap();
        assert!(!payload.is_empty());
    }

    #[test]
    fn test_build_fetch_sids() {
        let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
        let builder = RequestBuilder::new(model);

        let payload = builder.build_fetch_sids(&[60001, 60002]).unwrap();
        assert!(!payload.is_empty());
    }
}
