use std::collections::HashMap;

use coreconf_model::{CoreconfError, Result};

pub trait OperationBinding: Send + Sync {
    fn canonical_path(&self) -> &str;
    fn invoke(&self, input: Option<&serde_json::Value>) -> Result<Option<serde_json::Value>>;
}

#[derive(Default)]
pub struct OperationRegistry {
    bindings: HashMap<String, Box<dyn OperationBinding>>,
}

impl OperationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, binding: Box<dyn OperationBinding>) {
        self.bindings
            .insert(binding.canonical_path().to_string(), binding);
    }

    pub fn invoke(
        &self,
        canonical_path: &str,
        input: Option<&serde_json::Value>,
    ) -> Result<Option<serde_json::Value>> {
        let binding = self.bindings.get(canonical_path).ok_or_else(|| {
            CoreconfError::ResourceNotFound(format!("operation not found: {canonical_path}"))
        })?;
        binding.invoke(input)
    }
}
