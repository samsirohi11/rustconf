use coreconf_schema::CompiledSchemaBundle;

pub fn emit_bundle_json(bundle: &CompiledSchemaBundle) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(bundle)
}
