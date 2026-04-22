use coreconf_schema::CompiledSchemaBundle;

pub fn emit_yang(bundle: &CompiledSchemaBundle) -> String {
    let module = &bundle.modules[0];
    format!(
        "module {} {{\n  namespace \"urn:{}\";\n  prefix {};\n}}\n",
        module.name, module.name, module.name
    )
}
