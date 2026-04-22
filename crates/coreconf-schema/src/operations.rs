use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationField {
    pub name: String,
    pub yang_type: Option<crate::YangScalarType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationSchema {
    pub path: String,
    pub input: Vec<OperationField>,
    pub output: Vec<OperationField>,
}
