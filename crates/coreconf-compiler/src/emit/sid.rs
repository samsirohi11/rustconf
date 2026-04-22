use coreconf_schema::CompiledSchemaBundle;
use serde_json::json;

pub fn emit_sid_json(bundle: &CompiledSchemaBundle) -> Result<String, serde_json::Error> {
    let module = &bundle.modules[0];
    let items: Vec<_> = bundle
        .nodes
        .values()
        .enumerate()
        .map(|(index, node)| {
            json!({
                "namespace": "data",
                "identifier": node.path,
                "sid": node.sid.unwrap_or(60000 + index as i64 + 1),
            })
        })
        .collect();

    serde_json::to_string_pretty(&json!({
        "module-name": module.name,
        "module-revision": module.revision,
        "item": items,
    }))
}
