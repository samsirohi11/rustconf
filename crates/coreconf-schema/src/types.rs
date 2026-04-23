use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum YangScalarType {
    String,
    Boolean,
    Int64,
    Uint64,
    IdentityRef,
    LeafRef,
}
