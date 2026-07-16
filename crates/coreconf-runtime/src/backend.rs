pub trait Backend: Send + Sync {
    fn read_tree(&self) -> serde_json::Value;

    /// Replace the complete tree.
    ///
    /// A backend returning `Err` must leave the previously published tree
    /// unchanged.  Root iPATCH candidate atomicity relies on this contract.
    fn replace_tree(&mut self, next: serde_json::Value) -> coreconf_model::Result<()>;
}
