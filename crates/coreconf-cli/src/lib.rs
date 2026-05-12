//! CORECONF command-line interface
//!
//! Provides batch conversion, validation, and interactive shell workflows
//! for working with CORECONF/YANG SID artifacts and datastores.

pub mod commands;
pub mod complete;
pub mod session;

use coreconf_model::{CompositeModel, CoreconfError, SidFile};

/// Load a [`CompositeModel`] from one or more SID file paths.
///
/// Each path is read and parsed as a `.sid` JSON file, then merged into
/// a single composite model that spans all loaded modules.
pub fn load_model(sid_paths: &[String]) -> Result<CompositeModel, CliError> {
    if sid_paths.is_empty() {
        return Err(CliError::NoSidFiles);
    }

    let mut sid_files = Vec::with_capacity(sid_paths.len());
    for path in sid_paths {
        let sid_file = SidFile::from_file(path).map_err(|e| CliError::SidLoad(path.clone(), e))?;
        sid_files.push(sid_file);
    }

    CompositeModel::from_sid_files(sid_files).map_err(CliError::Model)
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("no SID files provided")]
    NoSidFiles,

    #[error("failed to load SID file '{0}': {1}")]
    SidLoad(String, CoreconfError),

    #[error("model error: {0}")]
    Model(#[from] CoreconfError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    InvalidInput(String),
}
