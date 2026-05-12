use coreconf_model::{CompositeModel, CoreconfError};
use coreconf_runtime::transport::coap_lite::CoreconfClient;
use coreconf_runtime::{
    encode_editable_value, read_editable_file, Backend, Datastore, EditableFormat, FileBackend,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::CliError;

/// An offline session backed by an in-memory datastore.
pub struct Session {
    datastore: Datastore,
}

#[derive(Debug, Clone, Copy)]
pub struct SaveOptions {
    pub create_backup: bool,
    pub force: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            create_backup: true,
            force: false,
        }
    }
}

pub struct FileSession {
    model: CompositeModel,
    backend: FileBackend,
    base_snapshot: Value,
    backup_created: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StagedChange {
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

impl FileSession {
    pub fn open(
        model: CompositeModel,
        path: impl Into<PathBuf>,
        format: EditableFormat,
    ) -> Result<Self, CliError> {
        let backend = FileBackend::open(model.clone(), path, format).map_err(CliError::Model)?;
        let base_snapshot = backend.read_tree();
        Ok(Self {
            model,
            backend,
            base_snapshot,
            backup_created: false,
        })
    }

    pub fn path(&self) -> &Path {
        self.backend.path()
    }

    pub fn model(&self) -> &CompositeModel {
        &self.model
    }

    pub fn format(&self) -> EditableFormat {
        self.backend.format()
    }

    pub fn get(&self, path: &str) -> Result<Option<Value>, CliError> {
        self.with_datastore(|datastore| datastore.get_path(path))
    }

    pub fn set(&mut self, path: &str, value: Value) -> Result<(), CliError> {
        self.with_datastore_mut(|datastore| datastore.set_path(path, value))
    }

    pub fn delete(&mut self, path: &str) -> Result<bool, CliError> {
        self.with_datastore_mut(|datastore| datastore.delete_path(path))
    }

    pub fn dump(&self) -> Value {
        self.backend.read_tree()
    }

    pub fn pending_patch(&self) -> Result<Vec<(String, Option<Value>)>, CliError> {
        diff_trees(&self.base_snapshot, &self.backend.read_tree()).map_err(CliError::Model)
    }

    pub fn staged_changes(&self) -> Result<Vec<StagedChange>, CliError> {
        Ok(self
            .pending_patch()?
            .into_iter()
            .map(|(path, after)| StagedChange {
                before: value_at_path(&self.base_snapshot, &path).cloned(),
                after,
                path,
            })
            .collect())
    }

    pub fn is_dirty(&self) -> Result<bool, CliError> {
        Ok(!self.pending_patch()?.is_empty())
    }

    pub fn reload(&mut self) -> Result<(), CliError> {
        let tree = read_editable_file(&self.model, self.backend.path(), self.backend.format())
            .map_err(CliError::Model)?;
        self.backend
            .replace_tree(tree.clone())
            .map_err(CliError::Model)?;
        self.base_snapshot = tree;
        Ok(())
    }

    pub fn save(&mut self, options: SaveOptions) -> Result<(), CliError> {
        self.validate_save(options.force)?;
        if options.create_backup && !self.backup_created && self.backend.path().exists() {
            std::fs::copy(self.backend.path(), backup_path(self.backend.path()))?;
            self.backup_created = true;
        }
        self.backend.save().map_err(CliError::Model)?;
        self.base_snapshot = self.backend.read_tree();
        Ok(())
    }

    pub fn encoded_working_copy(&self) -> Result<Vec<u8>, CliError> {
        encode_editable_value(
            &self.model,
            &self.backend.read_tree(),
            self.backend.format(),
        )
        .map_err(CliError::Model)
    }

    fn validate_save(&self, _force: bool) -> Result<(), CliError> {
        self.model
            .identifier_value_to_sid_value(self.backend.read_tree())
            .map(|_| ())
            .map_err(CliError::Model)
    }

    fn with_datastore<T>(
        &self,
        f: impl FnOnce(&Datastore) -> coreconf_model::Result<T>,
    ) -> Result<T, CliError> {
        let datastore = Datastore::with_backend(self.model.clone(), self.backend.clone());
        f(&datastore).map_err(CliError::Model)
    }

    fn with_datastore_mut<T>(
        &mut self,
        f: impl FnOnce(&mut Datastore) -> coreconf_model::Result<T>,
    ) -> Result<T, CliError> {
        let mut datastore = Datastore::with_backend(self.model.clone(), self.backend.clone());
        let result = f(&mut datastore).map_err(CliError::Model)?;
        self.backend
            .replace_tree(datastore.get_all())
            .map_err(CliError::Model)?;
        Ok(result)
    }
}

impl Session {
    /// Create a new session with an empty datastore.
    pub fn new(model: CompositeModel) -> Self {
        Self {
            datastore: Datastore::new_in_memory(model),
        }
    }

    /// Create a session pre-loaded with JSON data.
    pub fn with_json(model: CompositeModel, json: &str) -> Result<Self, CliError> {
        let data: Value = serde_json::from_str(json)?;
        Ok(Self {
            datastore: Datastore::with_backend(model, coreconf_runtime::MemoryBackend::new(data)),
        })
    }

    /// Get a value at the given predicate path.
    pub fn get(&self, path: &str) -> Result<Option<Value>, CliError> {
        self.datastore.get_path(path).map_err(CliError::Model)
    }

    /// Set a value at the given predicate path.
    pub fn set(&mut self, path: &str, value: Value) -> Result<(), CliError> {
        self.datastore
            .set_path(path, value)
            .map_err(CliError::Model)
    }

    /// Delete the value at the given predicate path.
    pub fn delete(&mut self, path: &str) -> Result<bool, CliError> {
        self.datastore.delete_path(path).map_err(CliError::Model)
    }

    /// Export the full datastore tree as JSON.
    pub fn dump(&self) -> Value {
        self.datastore.get_all()
    }

    /// Apply a batch of changes: set if value is `Some`, delete if `None`.
    pub fn apply_changes(&mut self, changes: &[(String, Option<Value>)]) -> Result<(), CliError> {
        self.datastore
            .apply_changes(changes)
            .map_err(CliError::Model)
    }

    /// Access the underlying datastore.
    pub fn datastore(&self) -> &Datastore {
        &self.datastore
    }
}

/// A live session stages local edits against a remote CORECONF snapshot.
pub struct LiveSession<C> {
    client: C,
    model: CompositeModel,
    base_snapshot: Value,
    working_copy: Datastore,
}

impl<C: CoreconfClient> LiveSession<C> {
    /// Fetch the current remote snapshot and start a staged working copy.
    pub fn new(model: CompositeModel, mut client: C) -> Result<Self, CliError> {
        let snapshot = client.fetch_snapshot().map_err(CliError::Model)?;
        Ok(Self::from_snapshot(model, client, snapshot))
    }

    /// Start a live session from an already-fetched snapshot.
    pub fn from_snapshot(model: CompositeModel, client: C, snapshot: Value) -> Self {
        let working_copy = Datastore::with_backend(
            model.clone(),
            coreconf_runtime::MemoryBackend::new(snapshot.clone()),
        );
        Self {
            client,
            model,
            base_snapshot: snapshot,
            working_copy,
        }
    }

    pub fn get(&self, path: &str) -> Result<Option<Value>, CliError> {
        self.working_copy.get_path(path).map_err(CliError::Model)
    }

    pub fn set(&mut self, path: &str, value: Value) -> Result<(), CliError> {
        self.working_copy
            .set_path(path, value)
            .map_err(CliError::Model)
    }

    pub fn delete(&mut self, path: &str) -> Result<bool, CliError> {
        self.working_copy.delete_path(path).map_err(CliError::Model)
    }

    pub fn pending_patch(&self) -> Result<Vec<(String, Option<Value>)>, CliError> {
        diff_trees(&self.base_snapshot, &self.working_copy.get_all()).map_err(CliError::Model)
    }

    /// Push staged changes to the remote server.
    pub fn push(&mut self) -> Result<(), CliError> {
        let remote_snapshot = self.client.fetch_snapshot().map_err(CliError::Model)?;
        if remote_snapshot != self.base_snapshot {
            return Err(CliError::Model(CoreconfError::ValidationError(
                "remote datastore changed since this live session was loaded".into(),
            )));
        }

        let patch = self.pending_patch()?;
        self.client.apply_patch(&patch).map_err(CliError::Model)?;
        self.base_snapshot = self.working_copy.get_all();
        Ok(())
    }

    /// Reload the working copy from the remote server.
    pub fn reload(&mut self) -> Result<(), CliError> {
        let snapshot = self.client.fetch_snapshot().map_err(CliError::Model)?;
        self.base_snapshot = snapshot.clone();
        self.working_copy = Datastore::with_backend(
            self.model.clone(),
            coreconf_runtime::MemoryBackend::new(snapshot),
        );
        Ok(())
    }
}

pub fn diff_trees(
    before: &Value,
    after: &Value,
) -> Result<Vec<(String, Option<Value>)>, CoreconfError> {
    let mut patch = Vec::new();
    diff_value(String::new(), before, after, &mut patch);
    patch.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(patch)
}
fn diff_value(
    path: String,
    before: &Value,
    after: &Value,
    patch: &mut Vec<(String, Option<Value>)>,
) {
    match (before, after) {
        (Value::Object(before_map), Value::Object(after_map)) => {
            for (key, before_value) in before_map {
                let child_path = join_json_path(&path, key);
                match after_map.get(key) {
                    Some(after_value) => diff_value(child_path, before_value, after_value, patch),
                    None => patch.push((child_path, None)),
                }
            }

            for (key, after_value) in after_map {
                if !before_map.contains_key(key) {
                    patch.push((join_json_path(&path, key), Some(after_value.clone())));
                }
            }
        }
        _ if before != after => {
            patch.push((normalize_root_path(&path), Some(after.clone())));
        }
        _ => {}
    }
}

fn join_json_path(parent: &str, key: &str) -> String {
    if parent.is_empty() || parent == "/" {
        format!("/{key}")
    } else {
        format!("{parent}/{key}")
    }
}

fn normalize_root_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

pub fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".bak");
    PathBuf::from(backup)
}

fn value_at_path<'a>(tree: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = tree;
    for segment in path.trim_matches('/').split('/') {
        if segment.is_empty() {
            continue;
        }
        current = current.as_object()?.get(segment)?;
    }
    Some(current)
}
