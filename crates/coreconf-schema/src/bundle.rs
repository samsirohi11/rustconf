use crate::{IdentitySchema, OperationSchema, SchemaNode, TypedefSchema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledSchemaBundle {
    pub format_version: u32,
    pub modules: Vec<SchemaModule>,
    #[serde(default)]
    pub typedefs: Vec<TypedefSchema>,
    #[serde(default)]
    pub identities: Vec<IdentitySchema>,
    pub nodes: BTreeMap<String, SchemaNode>,
    pub operations: BTreeMap<String, OperationSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaModule {
    pub name: String,
    pub revision: String,
}
