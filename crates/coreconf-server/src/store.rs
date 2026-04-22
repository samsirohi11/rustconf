use crate::audit::AuditEvent;
use serde_json::Value;

pub trait Store {
    fn write_snapshot(&mut self, schema_version: &str, snapshot: &Value) -> Result<(), String>;
    fn read_snapshot(&self, schema_version: &str) -> Result<Option<Value>, String>;
    fn append_audit(&mut self, event: AuditEvent) -> Result<(), String>;
    fn read_audit(&self) -> Result<Vec<AuditEvent>, String>;
}
