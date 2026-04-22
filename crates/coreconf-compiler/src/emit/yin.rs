use coreconf_schema::CompiledSchemaBundle;

pub fn emit_yin(bundle: &CompiledSchemaBundle) -> String {
    let module = &bundle.modules[0];
    format!(r#"<module name="{}"></module>"#, module.name)
}
