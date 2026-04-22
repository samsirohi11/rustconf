use serde_json::Value;
use std::collections::BTreeMap;

type OperationFn = Box<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

#[derive(Default)]
pub struct OperationRegistry {
    handlers: BTreeMap<String, OperationFn>,
}

impl OperationRegistry {
    pub fn register<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
    {
        self.handlers.insert(path.to_string(), Box::new(handler));
    }

    pub fn invoke(&self, path: &str, input: Value) -> Result<Value, String> {
        let handler = self
            .handlers
            .get(path)
            .ok_or_else(|| format!("unknown operation: {}", path))?;
        handler(input)
    }
}
