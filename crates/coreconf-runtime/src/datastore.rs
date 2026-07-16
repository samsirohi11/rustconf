use coreconf_model::instance_id::decode_instances;
use coreconf_model::{
    CompositeModel, CoreconfError, CoreconfModel, Instance, InstancePath, Result, YangType,
};
use serde_json::{Map, Value};

use crate::backend::Backend;
use crate::memory_backend::MemoryBackend;
use crate::path::PredicatePath;

pub struct Datastore {
    model: CompositeModel,
    backend: Box<dyn Backend>,
}

impl Datastore {
    pub fn new(model: CoreconfModel) -> Self {
        Self::new_in_memory(model.composite_model().clone())
    }

    pub fn with_data(model: CoreconfModel, data: Value) -> Self {
        Self::with_backend(model.composite_model().clone(), MemoryBackend::new(data))
    }

    pub fn from_json(model: CoreconfModel, json: &str) -> Result<Self> {
        let data: Value = serde_json::from_str(json)?;
        Ok(Self::with_data(model, data))
    }

    pub fn from_json_with_model(model: CompositeModel, json: &str) -> Result<Self> {
        let data: Value = serde_json::from_str(json)?;
        Ok(Self::with_backend(model, MemoryBackend::new(data)))
    }

    /// Build a datastore from a CORECONF CBOR payload (e.g. a FETCH or GET
    /// response).  Decodes the SID-keyed CBOR into an identifier-keyed JSON
    /// tree and populates the datastore.  Used during device bootstrap and
    /// observation-notification handling.
    ///
    /// ## Example
    ///
    /// ```ignore
    /// let model = CoreconfModel::from_sid_file("model.sid")?;
    /// let cbor_payload: Vec<u8> = /* CoAP FETCH response */;
    /// let ds = Datastore::from_cbor(model, &cbor_payload)?;
    /// ```
    pub fn from_cbor(model: CoreconfModel, cbor: &[u8]) -> Result<Self> {
        let coreconf_value = coreconf_model::codec::cbor_to_json_value(cbor)?;
        let value = model
            .composite_model()
            .sid_value_to_identifier_value_preserve_sids(coreconf_value)?;
        Ok(Self::with_data(model, value))
    }

    /// Build a datastore from a yang-instances+cbor-seq payload (the response
    /// to a FETCH with Accept: 142).  Each instance's SID is resolved to an
    /// identifier path, and the value is set at that path.
    ///
    /// Instance IDs with list-key values navigate into keyed list entries.
    pub fn from_cbor_instance_seq(model: CoreconfModel, cbor: &[u8]) -> Result<Self> {
        let composite = model.composite_model().clone();
        let mut ds = Datastore::new_in_memory(composite);
        ds.apply_instance_seq(cbor)?;
        Ok(ds)
    }

    /// Apply a yang-instances+cbor-seq payload to an existing datastore.
    ///
    /// Each instance in the payload is decoded and its value is set at the
    /// resolved predicate path.  Instance IDs with key values navigate into
    /// keyed list entries.
    pub fn apply_instance_seq(&mut self, cbor: &[u8]) -> Result<()> {
        for instance in decode_instances(cbor)? {
            let Some(sid) = instance.path.absolute_sid() else {
                continue;
            };

            let key_values: Vec<Value> = instance
                .path
                .components
                .iter()
                .filter_map(|c| match c {
                    coreconf_model::instance_id::PathComponent::KeyValue(v) => Some(v.clone()),
                    _ => None,
                })
                .collect();

            if key_values.is_empty() {
                let identifier = self
                    .model
                    .get_identifier(sid)
                    .ok_or(CoreconfError::IdentifierNotFound(sid))?
                    .to_string();
                if let Some(value) = instance.value {
                    // Instance values from FETCH responses are in CORECONF
                    // (SID-keyed) format.  Convert to identifier keys, then
                    // unwrap the top-level container key so set_path doesn't
                    // double-nest it.
                    let id_value = self
                        .model
                        .sid_value_to_identifier_value_preserve_sids(value)?;
                    let leaf = identifier.rsplit('/').next().unwrap_or(&identifier);
                    let unwrapped = id_value.get(leaf).cloned().unwrap_or(id_value);
                    self.set_path(&identifier, unwrapped)?;
                } else {
                    self.delete_path(&identifier)?;
                }
            } else {
                let xpath = self.create_xpath(sid, &key_values)?;
                if let Some(value) = instance.value {
                    // For leaf values inside keyed lists, the value is
                    // typically a scalar or small map — no unwrapping needed.
                    let id_value = self
                        .model
                        .sid_value_to_identifier_value_preserve_sids(value)?;
                    self.set_path(&xpath, id_value)?;
                } else {
                    self.delete_path(&xpath)?;
                }
            }
        }
        Ok(())
    }

    /// Replace the entire datastore tree with a decoded CBOR payload.
    ///
    /// This is used for observe notifications where each response carries
    /// a complete replacement of a subtree (e.g. history time-series).
    pub fn replace_from_cbor(&mut self, cbor: &[u8]) -> Result<()> {
        let coreconf_value = coreconf_model::codec::cbor_to_json_value(cbor)?;
        let value = self
            .model
            .sid_value_to_identifier_value_preserve_sids(coreconf_value)?;
        self.backend.replace_tree(value)
    }

    pub fn new_in_memory(model: CompositeModel) -> Self {
        Self {
            model,
            backend: Box::new(MemoryBackend::default()),
        }
    }

    pub fn with_backend(model: CompositeModel, backend: impl Backend + 'static) -> Self {
        Self {
            model,
            backend: Box::new(backend),
        }
    }

    pub fn model(&self) -> &CompositeModel {
        &self.model
    }

    pub fn get_all(&self) -> Value {
        self.backend.read_tree()
    }

    /// Publish a complete datastore tree through the configured backend.
    pub fn replace_tree(&mut self, tree: Value) -> Result<()> {
        self.backend.replace_tree(tree)
    }

    pub fn get_all_cbor(&self) -> Result<Vec<u8>> {
        encode_identifier_value_to_cbor(&self.model, &self.backend.read_tree())
    }

    pub fn get_by_sid(&self, sid: i64) -> Result<Option<Value>> {
        let identifier = self
            .model
            .get_identifier(sid)
            .ok_or(CoreconfError::IdentifierNotFound(sid))?;
        self.get_path(identifier)
    }

    pub fn get_by_path(&self, path: &str) -> Result<Option<Value>> {
        self.get_path(path)
    }

    pub fn get_path(&self, path: &str) -> Result<Option<Value>> {
        let parsed = PredicatePath::parse(path)?;
        let tree = self.backend.read_tree();
        let segments = split_canonical_segments(&parsed.canonical_path);
        let mut predicate_index = 0usize;
        let value = get_at_path(
            &tree,
            &self.model,
            &segments,
            0,
            String::new(),
            &parsed.predicates,
            &mut predicate_index,
        )?;
        if predicate_index != parsed.predicates.len() {
            return Err(CoreconfError::ValidationError(format!(
                "unused predicates in path '{path}'"
            )));
        }
        Ok(value)
    }

    pub fn set_by_sid(&mut self, sid: i64, value: Value) -> Result<()> {
        let identifier = self
            .model
            .get_identifier(sid)
            .ok_or(CoreconfError::IdentifierNotFound(sid))?
            .to_string();
        self.set_path(&identifier, value)
    }

    pub fn set_by_path(&mut self, path: &str, value: Value) -> Result<()> {
        self.set_path(path, value)
    }

    pub fn set_path(&mut self, path: &str, value: Value) -> Result<()> {
        let parsed = PredicatePath::parse(path)?;
        let mut tree = self.backend.read_tree();

        if parsed.canonical_path == "/" {
            tree = value;
        } else {
            let segments = split_canonical_segments(&parsed.canonical_path);
            let mut predicate_index = 0usize;
            let mut ctx = TreeCtx {
                model: &self.model,
                segments: &segments,
                predicates: &parsed.predicates,
                predicate_index: &mut predicate_index,
            };
            set_at_path(&mut tree, &mut ctx, 0, String::new(), value)?;
            if predicate_index != parsed.predicates.len() {
                return Err(CoreconfError::ValidationError(format!(
                    "unused predicates in path '{path}'"
                )));
            }
        }

        self.backend.replace_tree(tree)
    }

    pub fn delete_by_sid(&mut self, sid: i64) -> Result<bool> {
        let identifier = self
            .model
            .get_identifier(sid)
            .ok_or(CoreconfError::IdentifierNotFound(sid))?
            .to_string();
        self.delete_path(&identifier)
    }

    pub fn delete_by_path(&mut self, path: &str) -> Result<bool> {
        self.delete_path(path)
    }

    pub fn delete_path(&mut self, path: &str) -> Result<bool> {
        let parsed = PredicatePath::parse(path)?;
        if parsed.canonical_path == "/" {
            self.backend
                .replace_tree(Value::Object(Map::new()))
                .map(|_| true)
        } else {
            let mut tree = self.backend.read_tree();
            let segments = split_canonical_segments(&parsed.canonical_path);
            let mut predicate_index = 0usize;
            let deleted = delete_at_path(
                &mut tree,
                &self.model,
                &segments,
                0,
                String::new(),
                &parsed.predicates,
                &mut predicate_index,
            )?;
            if predicate_index != parsed.predicates.len() {
                return Err(CoreconfError::ValidationError(format!(
                    "unused predicates in path '{path}'"
                )));
            }
            self.backend.replace_tree(tree)?;
            Ok(deleted)
        }
    }

    pub fn delete(&mut self, path: &InstancePath) -> Result<bool> {
        if let Some(sid) = path.absolute_sid() {
            self.delete_by_sid(sid)
        } else if path.is_empty() {
            self.delete_path("/")
        } else {
            Ok(false)
        }
    }

    pub fn get(&self, path: &InstancePath) -> Result<Option<Value>> {
        if let Some(sid) = path.absolute_sid() {
            self.get_by_sid(sid)
        } else if path.is_empty() {
            Ok(Some(self.backend.read_tree()))
        } else {
            Ok(None)
        }
    }

    pub fn set(&mut self, path: &InstancePath, value: Value) -> Result<()> {
        if let Some(sid) = path.absolute_sid() {
            self.set_by_sid(sid, value)
        } else if path.is_empty() {
            self.backend.replace_tree(value)
        } else {
            Err(CoreconfError::ResourceNotFound("invalid path".into()))
        }
    }

    pub fn apply_changes(&mut self, changes: &[(String, Option<Value>)]) -> Result<()> {
        for (path, value) in changes {
            match value {
                Some(value) => self.set_path(path, value.clone())?,
                None => {
                    self.delete_path(path)?;
                }
            }
        }
        Ok(())
    }

    pub fn fetch_instances(&self, payload: &[u8]) -> Result<Vec<Instance>> {
        let mut instances = Vec::new();
        for path in decode_instances(payload)? {
            if let Some(sid) = path.path.absolute_sid()
                && let Some(identifier) = self.model.get_identifier(sid)
                && let Some(value) = self.get_path(identifier)?
            {
                let mut result_path = InstancePath::new();
                result_path.push_delta(sid);
                instances.push(Instance::new(result_path, value));
            }
        }
        Ok(instances)
    }

    pub fn encode_instances(&self, instances: &[Instance]) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        for inst in instances {
            let xpath = if let Some(sid) = inst.path.absolute_sid() {
                self.model
                    .get_identifier(sid)
                    .map(|id| id.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };

            let sid_value = match &inst.value {
                Some(value) => self
                    .model
                    .identifier_value_to_sid_value_at_path(value.clone(), &xpath)?,
                None => Value::Null,
            };

            let mut inst_map = serde_json::Map::new();
            let sid = inst.path.absolute_sid().unwrap_or(0);
            inst_map.insert(sid.to_string(), sid_value);
            let inst_json = Value::Object(inst_map);

            let ciborium_val =
                coreconf_model::codec::json_to_cbor_value(&self.model, &inst_json, 0);
            ciborium::into_writer(&ciborium_val, &mut bytes)
                .map_err(|e| CoreconfError::CborEncode(e.to_string()))?;
        }
        Ok(bytes)
    }

    /// Return list-key predicate strings for entries under a list XPath.
    ///
    /// Example: `ds.predicates("/transducers/transducer")` returns
    /// `["[type='coreconf-m2m:solar-radiation'][id='0']", ...]`.
    pub fn predicates(&self, path: &str) -> Result<Vec<String>> {
        let (list_sid, existing_keys) = self.resolve_xpath(path)?;
        let key_sids = self.model.get_keys(list_sid).cloned().ok_or_else(|| {
            CoreconfError::ValidationError(format!("path is not a keyed list: '{path}'"))
        })?;

        // If the XPath already includes predicates, return that single filter.
        if !existing_keys.is_empty() {
            let predicate_str = format_predicate_string(&self.model, &key_sids, &existing_keys)?;
            return Ok(vec![predicate_str]);
        }

        // Enumerate all entries in the list.
        let tree = self.backend.read_tree();
        let segments = split_canonical_segments(path);

        let list_value =
            match get_at_path(&tree, &self.model, &segments, 0, String::new(), &[], &mut 0)? {
                Some(val) => val,
                None => return Ok(Vec::new()),
            };

        let list_name = segments.last().copied().unwrap_or("");
        let storage_key = storage_key(list_name, segments.len() - 1);
        let entries = list_value
            .as_object()
            .and_then(|map| map.get(&storage_key))
            .and_then(Value::as_array)
            .map(|arr| arr.to_vec())
            .unwrap_or_default();

        let mut result = Vec::with_capacity(entries.len());
        for entry in &entries {
            let entry_obj = match entry.as_object() {
                Some(obj) => obj,
                None => continue,
            };

            let mut values = Vec::with_capacity(key_sids.len());
            let mut complete = true;
            for key_sid in &key_sids {
                let key_identifier = self
                    .model
                    .get_identifier(*key_sid)
                    .ok_or(CoreconfError::IdentifierNotFound(*key_sid))?;
                let key_leaf = segment_leaf(key_identifier);
                match entry_obj.get(key_leaf) {
                    Some(val) => values.push(val.clone()),
                    None => {
                        complete = false;
                        break;
                    }
                }
            }
            if complete {
                let predicate_str = format_predicate_string(&self.model, &key_sids, &values)?;
                result.push(predicate_str);
            }
        }

        Ok(result)
    }

    /// Resolve an XPath string to (target SID, key values).
    ///
    /// This is the inverse of `create_xpath`.
    pub fn resolve_xpath(&self, path: &str) -> Result<(i64, Vec<Value>)> {
        let parsed = PredicatePath::parse(path)?;
        let target_sid = self
            .model
            .get_sid(&parsed.canonical_path)
            .ok_or_else(|| CoreconfError::SidNotFound(parsed.canonical_path.clone()))?;

        let mut key_values = Vec::new();
        if !parsed.predicates.is_empty() {
            // Walk through the canonical path segments to find list ancestors
            // and consume predicates at the correct list node.
            let segments = split_canonical_segments(&parsed.canonical_path);
            let mut current_path = String::new();
            let mut predicate_index = 0usize;

            for segment in segments.iter() {
                current_path = join_path(&current_path, segment);
                let list_entry_keys = list_keys(&self.model, &current_path)?;
                if !list_entry_keys.is_empty() {
                    let consumed = consume_key_values(
                        &self.model,
                        &list_entry_keys,
                        &parsed.predicates,
                        &mut predicate_index,
                    )?;
                    key_values.extend(consumed.into_iter().map(|(_, v)| v));
                }
            }

            if predicate_index != parsed.predicates.len() {
                return Err(CoreconfError::ValidationError(format!(
                    "unused predicates in path '{path}'"
                )));
            }
        }

        Ok((target_sid, key_values))
    }

    /// Convert a target SID and optional key values back to an XPath string.
    ///
    /// This is the inverse of `resolve_xpath`.  The returned path preserves
    /// the module prefix on the first segment so it can be used directly with
    /// `get_path` and `set_path`.
    pub fn create_xpath(&self, sid: i64, keys: &[Value]) -> Result<String> {
        let identifier = self
            .model
            .get_identifier(sid)
            .ok_or(CoreconfError::IdentifierNotFound(sid))?;

        let segments: Vec<&str> = identifier
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut xpath_parts = Vec::with_capacity(segments.len());
        let mut current_path = String::new();
        let mut key_index = 0usize;

        for (depth, segment) in segments.iter().enumerate() {
            // Depth 0 keeps the module prefix (e.g. "coreconf-m2m:transducers"),
            // deeper levels use just the leaf name since that's how the datastore
            // stores keys.
            let name = if depth == 0 {
                *segment
            } else {
                segment.rsplit(':').next().unwrap_or(segment)
            };
            current_path = if current_path.is_empty() {
                format!("/{segment}")
            } else {
                format!("{current_path}/{segment}")
            };

            let seg_sid = self.model.get_sid(&current_path);
            let is_list = seg_sid.and_then(|sid| self.model.get_keys(sid)).is_some();

            if is_list {
                let list_sid = seg_sid.unwrap();
                let key_sids = self.model.get_keys(list_sid).unwrap();
                let mut predicates = Vec::with_capacity(key_sids.len());

                for key_sid in key_sids {
                    if key_index >= keys.len() {
                        break;
                    }
                    let key_value = &keys[key_index];
                    key_index += 1;

                    let key_identifier = self
                        .model
                        .get_identifier(*key_sid)
                        .ok_or(CoreconfError::IdentifierNotFound(*key_sid))?;
                    let key_name = segment_leaf(key_identifier);

                    let formatted = format_key_value(&self.model, key_identifier, key_value)?;
                    predicates.push(format!("[{key_name}='{formatted}']"));
                }

                if !predicates.is_empty() {
                    xpath_parts.push(format!("{name}{}", predicates.concat()));
                } else {
                    xpath_parts.push(name.to_string());
                }
            } else {
                xpath_parts.push(name.to_string());
            }
        }

        Ok(format!("/{}", xpath_parts.join("/")))
    }
}

fn encode_identifier_value_to_cbor(model: &CompositeModel, value: &Value) -> Result<Vec<u8>> {
    let sid_value = model.identifier_value_to_sid_value(value.clone())?;
    let ciborium_val = coreconf_model::codec::json_to_cbor_value(model, &sid_value, 0);
    let mut bytes = Vec::new();
    ciborium::into_writer(&ciborium_val, &mut bytes)
        .map_err(|error| CoreconfError::CborEncode(error.to_string()))?;
    Ok(bytes)
}

fn split_canonical_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn get_at_path(
    current: &Value,
    model: &CompositeModel,
    segments: &[&str],
    depth: usize,
    current_path: String,
    predicates: &[(String, String)],
    predicate_index: &mut usize,
) -> Result<Option<Value>> {
    if depth == segments.len() {
        return Ok(Some(current.clone()));
    }

    let segment = segments[depth];
    let next_path = join_path(&current_path, segment);
    let list_keys = list_keys(model, &next_path)?;

    if list_keys.is_empty() {
        let map = match current.as_object() {
            Some(map) => map,
            None => return Ok(None),
        };
        let storage_key = storage_key(segment, depth);
        let child = map
            .get(&storage_key)
            .or_else(|| map.get(segment_leaf(segment)))
            .or_else(|| map.get(segment));
        match child {
            Some(child) => get_at_path(
                child,
                model,
                segments,
                depth + 1,
                next_path,
                predicates,
                predicate_index,
            ),
            None => Ok(None),
        }
    } else {
        let end = *predicate_index + list_keys.len();
        let has_predicates = end <= predicates.len();
        let is_last = depth == segments.len() - 1;

        if !has_predicates && !is_last {
            return Err(CoreconfError::ValidationError(format!(
                "predicates required for keyed list node '{segment}' in path '{current_path}/{segment}'"
            )));
        }

        if !has_predicates && is_last {
            // Last segment is a list node with no predicates — return current
            // value as-is (used by predicates() to read the entire list).
            return Ok(Some(current.clone()));
        }

        let key_values = consume_key_values(model, &list_keys, predicates, predicate_index)?;
        let map = match current.as_object() {
            Some(map) => map,
            None => return Ok(None),
        };
        let storage_key = storage_key(segment, depth);
        let list = match map.get(&storage_key).and_then(Value::as_array) {
            Some(list) => list,
            None => return Ok(None),
        };
        let entry = list
            .iter()
            .find(|value| list_entry_matches(value, &key_values, model, &next_path))
            .cloned();
        match entry {
            Some(entry) => get_at_path(
                &entry,
                model,
                segments,
                depth + 1,
                next_path,
                predicates,
                predicate_index,
            ),
            None => Ok(None),
        }
    }
}

/// Shared context for tree traversal functions, coalescing common arguments.
struct TreeCtx<'a, 'b> {
    model: &'a CompositeModel,
    segments: &'a [&'a str],
    predicates: &'a [(String, String)],
    predicate_index: &'b mut usize,
}

fn set_at_path(
    current: &mut Value,
    ctx: &mut TreeCtx<'_, '_>,
    depth: usize,
    current_path: String,
    value: Value,
) -> Result<()> {
    let segment = ctx.segments[depth];
    let next_path = join_path(&current_path, segment);
    let list_keys = list_keys(ctx.model, &next_path)?;

    if list_keys.is_empty() {
        let map = ensure_object(current)?;
        let key = storage_key(segment, depth);
        if depth == ctx.segments.len() - 1 {
            map.insert(key, value);
            return Ok(());
        }

        let child = map.entry(key).or_insert_with(|| Value::Object(Map::new()));
        set_at_path(child, ctx, depth + 1, next_path, value)
    } else {
        let end = *ctx.predicate_index + list_keys.len();
        let has_predicates = end <= ctx.predicates.len();
        let is_last = depth == ctx.segments.len() - 1;

        if !has_predicates && !is_last {
            return Err(CoreconfError::ValidationError(format!(
                "predicates required for keyed list node '{segment}' in path '{current_path}/{segment}'"
            )));
        }

        if !has_predicates && is_last {
            // Last segment is a list node with no predicates — replace the
            // entire list value (used when setting a full list array).
            let map = ensure_object(current)?;
            map.insert(storage_key(segment, depth), value);
            return Ok(());
        }

        let key_values =
            consume_key_values(ctx.model, &list_keys, ctx.predicates, ctx.predicate_index)?;
        let map = ensure_object(current)?;
        let list = map
            .entry(storage_key(segment, depth))
            .or_insert_with(|| Value::Array(Vec::new()));
        let entries = ensure_array(list)?;
        let entry = find_or_create_list_entry(entries, &key_values, ctx.model, &next_path);

        if depth == ctx.segments.len() - 1 {
            let mut next_value = value;
            if let Value::Object(map) = &mut next_value {
                for (key, key_value) in &key_values {
                    map.entry(key.clone()).or_insert_with(|| key_value.clone());
                }
            }
            *entry = next_value;
            return Ok(());
        }

        set_at_path(entry, ctx, depth + 1, next_path, value)
    }
}

fn delete_at_path(
    current: &mut Value,
    model: &CompositeModel,
    segments: &[&str],
    depth: usize,
    current_path: String,
    predicates: &[(String, String)],
    predicate_index: &mut usize,
) -> Result<bool> {
    let segment = segments[depth];
    let next_path = join_path(&current_path, segment);
    let list_keys = list_keys(model, &next_path)?;

    if list_keys.is_empty() {
        let map = match current.as_object_mut() {
            Some(map) => map,
            None => return Ok(false),
        };
        let key = storage_key(segment, depth);
        if depth == segments.len() - 1 {
            return Ok(map.remove(&key).is_some());
        }

        match map.get_mut(&key) {
            Some(child) => delete_at_path(
                child,
                model,
                segments,
                depth + 1,
                next_path,
                predicates,
                predicate_index,
            ),
            None => Ok(false),
        }
    } else {
        let key_values = consume_key_values(model, &list_keys, predicates, predicate_index)?;
        let map = match current.as_object_mut() {
            Some(map) => map,
            None => return Ok(false),
        };
        let list = match map
            .get_mut(&storage_key(segment, depth))
            .and_then(Value::as_array_mut)
        {
            Some(list) => list,
            None => return Ok(false),
        };

        let position = list
            .iter()
            .position(|entry| list_entry_matches(entry, &key_values, model, &next_path));

        let Some(position) = position else {
            return Ok(false);
        };

        if depth == segments.len() - 1 {
            list.remove(position);
            return Ok(true);
        }

        delete_at_path(
            &mut list[position],
            model,
            segments,
            depth + 1,
            next_path,
            predicates,
            predicate_index,
        )
    }
}

fn list_keys(model: &CompositeModel, list_path: &str) -> Result<Vec<(String, Value)>> {
    let Some(list_sid) = model.get_sid(list_path) else {
        return Err(CoreconfError::SidNotFound(list_path.to_string()));
    };

    let Some(key_sids) = model.get_keys(list_sid) else {
        return Ok(Vec::new());
    };

    let mut keys = Vec::with_capacity(key_sids.len());
    for key_sid in key_sids {
        let identifier = model
            .get_identifier(*key_sid)
            .ok_or(CoreconfError::IdentifierNotFound(*key_sid))?;
        keys.push((
            segment_leaf(identifier).to_string(),
            Value::String(identifier.to_string()),
        ));
    }
    Ok(keys)
}

fn consume_key_values(
    model: &CompositeModel,
    expected_keys: &[(String, Value)],
    predicates: &[(String, String)],
    predicate_index: &mut usize,
) -> Result<Vec<(String, Value)>> {
    let start = *predicate_index;
    let end = start + expected_keys.len();
    if end > predicates.len() {
        return Err(CoreconfError::ValidationError(
            "missing predicate values for keyed list".into(),
        ));
    }

    let mut values = Vec::with_capacity(expected_keys.len());
    let predicate_slice = &predicates[start..end];

    if expected_keys.len() == 1 {
        for ((expected_name, identifier_value), (actual_name, actual_value)) in
            expected_keys.iter().zip(predicate_slice)
        {
            let identifier = identifier_value.as_str().unwrap_or_default();
            if !predicate_name_matches(expected_name, identifier, actual_name) {
                return Err(CoreconfError::ValidationError(format!(
                    "predicate '{actual_name}' does not match expected key '{expected_name}'"
                )));
            }
            values.push((
                expected_name.clone(),
                coerce_predicate_value(model, identifier, actual_value)?,
            ));
        }
    } else {
        let mut matched = vec![false; predicate_slice.len()];

        for (expected_name, identifier_value) in expected_keys {
            let identifier = identifier_value.as_str().unwrap_or_default();
            let Some((matched_index, (_, actual_value))) =
                predicate_slice
                    .iter()
                    .enumerate()
                    .find(|(index, (actual_name, _))| {
                        !matched[*index]
                            && predicate_name_matches(expected_name, identifier, actual_name)
                    })
            else {
                return Err(CoreconfError::ValidationError(format!(
                    "missing predicate for expected key '{expected_name}'"
                )));
            };

            matched[matched_index] = true;
            values.push((
                expected_name.clone(),
                coerce_predicate_value(model, identifier, actual_value)?,
            ));
        }

        if let Some((_, (actual_name, _))) = predicate_slice
            .iter()
            .enumerate()
            .find(|(index, _)| !matched[*index])
        {
            return Err(CoreconfError::ValidationError(format!(
                "predicate '{actual_name}' does not match any expected key"
            )));
        }
    }

    *predicate_index = end;
    Ok(values)
}

fn coerce_predicate_value(model: &CompositeModel, identifier: &str, raw: &str) -> Result<Value> {
    match model.get_type(identifier) {
        Some(YangType::Boolean) => match raw {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(CoreconfError::TypeConversion(format!(
                "cannot parse '{raw}' as boolean"
            ))),
        },
        Some(YangType::Identityref) => {
            // Accept the identity name (e.g. "coreconf-m2m:solar-radiation") or raw SID.
            if let Ok(sid) = raw.parse::<i64>() {
                return Ok(Value::Number(sid.into()));
            }
            // Try exact match, then with leading /, then unambiguous unqualified name.
            let sid = model
                .get_sid(raw)
                .or_else(|| model.get_sid(&format!("/{raw}")))
                .or_else(|| resolve_unqualified_identity(model, raw))
                .ok_or_else(|| {
                    CoreconfError::TypeConversion(format!(
                        "identityref predicate value not found: '{raw}'"
                    ))
                })?;
            Ok(Value::Number(sid.into()))
        }
        Some(YangType::Enumeration(enum_map)) => {
            // Accept the enum name (e.g. "delta") or raw integer.
            if let Ok(int_val) = raw.parse::<i64>() {
                return Ok(Value::Number(int_val.into()));
            }
            // Reverse lookup: find the integer value whose name matches.
            let (_, int_val) = enum_map
                .iter()
                .find(|(name, _)| name.as_str() == raw)
                .ok_or_else(|| {
                    CoreconfError::TypeConversion(format!(
                        "enum predicate value not found: '{raw}'"
                    ))
                })?;
            Ok(Value::Number((*int_val).into()))
        }
        Some(YangType::Int8 | YangType::Int16 | YangType::Int32 | YangType::Int64) => raw
            .parse::<i64>()
            .map(|value| Value::Number(value.into()))
            .map_err(|_| CoreconfError::TypeConversion(format!("cannot parse '{raw}' as integer"))),
        Some(YangType::Uint8 | YangType::Uint16 | YangType::Uint32 | YangType::Uint64) => raw
            .parse::<u64>()
            .map(|value| Value::Number(value.into()))
            .map_err(|_| {
                CoreconfError::TypeConversion(format!("cannot parse '{raw}' as unsigned integer"))
            }),
        Some(YangType::Decimal64) => {
            let number = raw.parse::<f64>().map_err(|_| {
                CoreconfError::TypeConversion(format!("cannot parse '{raw}' as decimal64"))
            })?;
            serde_json::Number::from_f64(number)
                .map(Value::Number)
                .ok_or_else(|| {
                    CoreconfError::TypeConversion(format!("cannot represent '{raw}' as decimal64"))
                })
        }
        _ => Ok(Value::String(raw.to_string())),
    }
}

/// Resolve an unqualified identity name (e.g. "solar-radiation") when unique across modules.
fn resolve_unqualified_identity(model: &CompositeModel, short_name: &str) -> Option<i64> {
    let mut matches = Vec::new();
    for (identifier, sid) in &model.sids {
        // Only consider top-level qualified names (module_name:identity).
        if identifier.contains('/') || !identifier.contains(':') {
            continue;
        }
        let candidate_short = identifier.split(':').next_back().unwrap_or(identifier);
        if candidate_short == short_name {
            matches.push(*sid);
        }
    }
    (matches.len() == 1).then_some(matches[0])
}

fn predicate_name_matches(expected_leaf: &str, identifier: &str, actual_name: &str) -> bool {
    actual_name == expected_leaf
        || actual_name == identifier
        || actual_name == segment_leaf(identifier)
}

/// Compare a list entry against key values, handling identityref flexibly.
///
/// Matches pycoreconf's `_walk` entry comparison: identityref values may be
/// stored as string names (e.g. `"coreconf-m2m:solar-radiation"`) while
/// predicate values are coerced to SID numbers.  This function resolves both
/// representations to the same type before comparing.
fn list_entry_matches(
    entry: &Value,
    key_values: &[(String, Value)],
    model: &CompositeModel,
    list_identifier: &str,
) -> bool {
    let Some(map) = entry.as_object() else {
        return false;
    };
    key_values.iter().all(|(name, value)| {
        let stored = map.get(name);
        let is_match = stored.is_some_and(|stored| stored == value);
        if is_match {
            return true;
        }
        // Flexible identityref comparison: pycoreconf stores identityref as SID integers
        // and compares them directly.  In coreconf the internal tree uses string names from
        // decode_cbor_to_json, so stored values may differ in representation from predicate
        // values.  Resolve both to SIDs before comparing.
        let Some(stored) = stored else {
            return false;
        };
        identityref_equal(model, list_identifier, name, stored, value)
    })
}

/// Compare two identityref values that may differ in representation.
///
/// - Predicate values are always SID numbers (coerced by `coerce_predicate_value`).
/// - Stored values are typically string names (from `decode_cbor_to_json`).
///
/// Resolves string names to SIDs before comparing, mirroring pycoreconf's
/// `_resolve_identity_to_sid` which converts predicate names to SIDs at parse time
/// while the stored representation is already a SID integer.
fn identityref_equal(
    model: &CompositeModel,
    list_identifier: &str,
    key_name: &str,
    stored: &Value,
    predicate: &Value,
) -> bool {
    // Determine the full YANG identifier for this key leaf.
    let key_identifier = format!("{list_identifier}/{key_name}");

    // Only apply flexible comparison when the key leaf is identityref-typed.
    if !matches!(model.get_type(&key_identifier), Some(YangType::Identityref)) {
        return false;
    }

    // Normalise both values to an i64 SID.
    let stored_sid = value_to_identityref_sid(stored, model);
    let predicate_sid = value_to_identityref_sid(predicate, model);

    stored_sid.is_some_and(|s| predicate_sid.is_some_and(|p| s == p))
}

/// Convert a `serde_json::Value` to an identityref SID, accepting both
/// numeric SIDs and string identity names (matching pycoreconf's
/// `_resolve_identity_to_sid`).
fn value_to_identityref_sid(value: &Value, model: &CompositeModel) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => {
            // Exact match (e.g. "coreconf-m2m:solar-radiation").
            if let Some(sid) = model.get_sid(s) {
                return Some(sid);
            }
            // With leading slash (e.g. "/coreconf-m2m:solar-radiation").
            let prefixed = if s.starts_with('/') {
                s.clone()
            } else {
                format!("/{s}")
            };
            if let Some(sid) = model.get_sid(&prefixed) {
                return Some(sid);
            }
            // Unqualified name (e.g. "solar-radiation") — resolve only when
            // unambiguous across loaded modules.
            resolve_unqualified_identity(model, s)
        }
        _ => None,
    }
}

fn find_or_create_list_entry<'a>(
    entries: &'a mut Vec<Value>,
    key_values: &[(String, Value)],
    model: &CompositeModel,
    list_identifier: &str,
) -> &'a mut Value {
    if let Some(position) = entries
        .iter()
        .position(|entry| list_entry_matches(entry, key_values, model, list_identifier))
    {
        return &mut entries[position];
    }

    let mut map = Map::new();
    for (name, value) in key_values {
        map.insert(name.clone(), value.clone());
    }
    entries.push(Value::Object(map));
    entries.last_mut().expect("list entry was just inserted")
}

fn ensure_object(value: &mut Value) -> Result<&mut Map<String, Value>> {
    if value.is_null() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().ok_or_else(|| {
        CoreconfError::ValidationError("expected JSON object while traversing datastore".into())
    })
}

fn ensure_array(value: &mut Value) -> Result<&mut Vec<Value>> {
    if value.is_null() {
        *value = Value::Array(Vec::new());
    }
    value
        .as_array_mut()
        .ok_or_else(|| CoreconfError::ValidationError("expected JSON array for keyed list".into()))
}

fn join_path(current_path: &str, segment: &str) -> String {
    if current_path.is_empty() {
        format!("/{segment}")
    } else {
        format!("{current_path}/{segment}")
    }
}

fn storage_key(segment: &str, depth: usize) -> String {
    if depth == 0 {
        segment.to_string()
    } else {
        segment_leaf(segment).to_string()
    }
}

fn segment_leaf(segment: &str) -> &str {
    segment
        .rsplit('/')
        .next()
        .unwrap_or(segment)
        .split(':')
        .next_back()
        .unwrap_or(segment)
}

/// Format predicate string from key SIDs and values (e.g., "[type='solar-radiation'][id='0']").
fn format_predicate_string(
    model: &CompositeModel,
    key_sids: &[i64],
    key_values: &[Value],
) -> Result<String> {
    let mut parts = String::new();
    for (key_sid, key_value) in key_sids.iter().zip(key_values.iter()) {
        let key_identifier = model
            .get_identifier(*key_sid)
            .ok_or(CoreconfError::IdentifierNotFound(*key_sid))?;
        let key_name = segment_leaf(key_identifier);
        let formatted = format_key_value(model, key_identifier, key_value)?;
        parts.push_str(&format!("[{key_name}='{formatted}']"));
    }
    Ok(parts)
}

/// Format a key value for display in an XPath predicate string.
fn format_key_value(model: &CompositeModel, identifier: &str, value: &Value) -> Result<String> {
    match model.get_type(identifier) {
        Some(YangType::Identityref) => {
            // Accept both numeric SIDs (set_path-created entries) and
            // string identity names (CBOR-decoded data).
            let sid = match value {
                Value::Number(n) => n.as_i64(),
                Value::String(s) => model
                    .get_sid(s)
                    .or_else(|| model.get_sid(&format!("/{s}")))
                    .or_else(|| resolve_unqualified_identity(model, s)),
                _ => None,
            }
            .ok_or_else(|| {
                CoreconfError::TypeConversion(format!(
                    "expected integer SID or identity name for identityref, got {value}"
                ))
            })?;
            let identity = model
                .get_identifier(sid)
                .map(|id| id.trim_start_matches('/').to_string())
                .unwrap_or_else(|| sid.to_string());
            Ok(identity)
        }
        Some(YangType::Enumeration(enum_map)) => {
            // Convert numeric value to enum name.
            if let Some(num) = value.as_i64() {
                let key = num.to_string();
                // Reverse lookup: find the name that maps to this integer value.
                if let Some((name, _)) = enum_map.iter().find(|(_, v)| **v == num) {
                    return Ok(name.clone());
                }
                return Ok(key);
            }
            Ok(value.to_string())
        }
        _ => {
            // For string keys, strip quotes. For numeric keys, format as string.
            match value {
                Value::String(s) => Ok(s.clone()),
                Value::Number(n) => Ok(n.to_string()),
                Value::Bool(b) => Ok(b.to_string()),
                _ => Ok(value.to_string()),
            }
        }
    }
}
