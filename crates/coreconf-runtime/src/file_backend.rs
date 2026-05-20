use std::io::Write;
use std::path::{Path, PathBuf};

use coreconf_model::{CompositeModel, CoreconfError, Result};
use serde_json::Value;

use crate::backend::Backend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditableFormat {
    Json,
    Cbor,
}

impl EditableFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("json") => Some(Self::Json),
            Some("cbor") => Some(Self::Cbor),
            _ => None,
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "json" => Some(Self::Json),
            "cbor" => Some(Self::Cbor),
            _ => None,
        }
    }
}

impl std::fmt::Display for EditableFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json => f.write_str("json"),
            Self::Cbor => f.write_str("cbor"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileBackend {
    path: PathBuf,
    format: EditableFormat,
    model: CompositeModel,
    tree: Value,
}

impl FileBackend {
    pub fn open(
        model: CompositeModel,
        path: impl Into<PathBuf>,
        format: EditableFormat,
    ) -> Result<Self> {
        let path = path.into();
        let tree = read_editable_file(&model, &path, format)?;
        Ok(Self {
            path,
            format,
            model,
            tree,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn format(&self) -> EditableFormat {
        self.format
    }

    pub fn save(&self) -> Result<()> {
        self.save_as(&self.path, self.format)
    }

    pub fn save_as(&self, path: &Path, format: EditableFormat) -> Result<()> {
        let bytes = encode_editable_value(&self.model, &self.tree, format)?;
        atomic_write(path, &bytes)
    }
}

impl Backend for FileBackend {
    fn read_tree(&self) -> Value {
        self.tree.clone()
    }

    fn replace_tree(&mut self, next: Value) -> Result<()> {
        self.tree = next;
        Ok(())
    }
}

pub fn read_editable_file(
    model: &CompositeModel,
    path: &Path,
    format: EditableFormat,
) -> Result<Value> {
    match format {
        EditableFormat::Json => {
            let contents = std::fs::read_to_string(path)?;
            serde_json::from_str(&contents).map_err(CoreconfError::from)
        }
        EditableFormat::Cbor => {
            let bytes = std::fs::read(path)?;
            let json = coreconf_model::decode_cbor_to_json(model, &bytes)?;
            serde_json::from_str(&json).map_err(CoreconfError::from)
        }
    }
}

pub fn encode_editable_value(
    model: &CompositeModel,
    value: &Value,
    format: EditableFormat,
) -> Result<Vec<u8>> {
    match format {
        EditableFormat::Json => {
            let mut json = serde_json::to_string_pretty(value)?;
            json.push('\n');
            Ok(json.into_bytes())
        }
        EditableFormat::Cbor => {
            let sid_value = model.identifier_value_to_sid_value(value.clone())?;
            let ciborium_val = coreconf_model::codec::json_to_cbor_value(model, &sid_value, 0);
            let mut bytes = Vec::new();
            ciborium::into_writer(&ciborium_val, &mut bytes)
                .map_err(|error| CoreconfError::CborEncode(error.to_string()))?;
            Ok(bytes)
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp_file = tempfile::NamedTempFile::new_in(directory)?;
    temp_file.write_all(bytes)?;
    temp_file.flush()?;
    temp_file
        .persist(path)
        .map_err(|error| CoreconfError::Io(error.error))?;
    Ok(())
}
