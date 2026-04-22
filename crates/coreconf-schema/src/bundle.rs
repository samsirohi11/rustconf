use crate::{OperationSchema, SchemaNode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledSchemaBundle {
    pub format_version: u32,
    pub modules: Vec<SchemaModule>,
    pub nodes: BTreeMap<String, SchemaNode>,
    pub operations: BTreeMap<String, OperationSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaModule {
    pub name: String,
    pub revision: String,
}
