use crate::YangScalarType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeKind {
    Container,
    List,
    Leaf,
    LeafList,
    Rpc,
    Input,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaNode {
    pub path: String,
    pub sid: Option<i64>,
    pub kind: NodeKind,
    pub yang_type: Option<YangScalarType>,
    #[serde(default)]
    pub type_ref: Option<crate::ResolvedType>,
    pub keys: Vec<String>,
    pub children: Vec<String>,
    pub must: Vec<String>,
    pub when: Option<String>,
}
