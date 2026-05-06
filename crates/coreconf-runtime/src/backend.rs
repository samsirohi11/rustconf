pub trait Backend: Send + Sync {
    fn read_tree(&self) -> serde_json::Value;
    fn replace_tree(&mut self, next: serde_json::Value) -> coreconf_model::Result<()>;
}
