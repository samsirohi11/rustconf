use coreconf_schema::CompiledSchemaBundle;

pub fn emit_tree(bundle: &CompiledSchemaBundle) -> String {
    let mut lines = bundle.nodes.keys().cloned().collect::<Vec<_>>();
    lines.sort();
    lines.join("\n")
}
