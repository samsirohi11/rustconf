use crate::audit::AuditEvent;
use coreconf_schema::CompiledSchemaBundle;
use serde_json::Value;

pub trait Store {
    fn write_bundle(
        &mut self,
        schema_version: &str,
        bundle: &CompiledSchemaBundle,
    ) -> Result<(), String>;
    fn read_bundle(&self, schema_version: &str) -> Result<Option<CompiledSchemaBundle>, String>;
    fn set_active_schema_version(&mut self, schema_version: &str) -> Result<(), String>;
    fn active_schema_version(&self) -> Result<Option<String>, String>;
    fn write_snapshot(&mut self, schema_version: &str, snapshot: &Value) -> Result<(), String>;
    fn read_snapshot(&self, schema_version: &str) -> Result<Option<Value>, String>;
    fn append_audit(&mut self, event: AuditEvent) -> Result<(), String>;
    fn read_audit(&self) -> Result<Vec<AuditEvent>, String>;
}
