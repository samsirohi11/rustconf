use serde_json::{Map, Value};

use crate::backend::Backend;

#[derive(Debug, Clone)]
pub struct MemoryBackend {
    tree: Value,
}

impl MemoryBackend {
    pub fn new(tree: Value) -> Self {
        Self { tree }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new(Value::Object(Map::new()))
    }
}

impl Backend for MemoryBackend {
    fn read_tree(&self) -> Value {
        self.tree.clone()
    }

    fn replace_tree(&mut self, next: Value) -> coreconf_model::Result<()> {
        self.tree = next;
        Ok(())
    }
}
