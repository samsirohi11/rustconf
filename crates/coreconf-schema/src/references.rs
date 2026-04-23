use crate::YangScalarType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedefSchema {
    pub module: String,
    pub name: String,
    pub base: YangScalarType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentitySchema {
    pub module: String,
    pub name: String,
    pub base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResolvedType {
    Typedef { name: String, base: YangScalarType },
    IdentityRef { base: String, allowed: Vec<String> },
    LeafRef { target_path: String },
}
