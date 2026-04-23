use coreconf_schema::CompiledSchemaBundle;

pub fn emit_yang(bundle: &CompiledSchemaBundle) -> String {
    let Some(module) = primary_module(bundle) else {
        return String::new();
    };

    let mut out = format!(
        "module {} {{\n  namespace \"urn:{}\";\n  prefix {};\n",
        module.name, module.name, module.name
    );

    for path in top_level_paths(bundle, &module.name) {
        render_yang_node(&mut out, path, bundle, 1);
    }

    out.push_str("}\n");
    out
}

fn primary_module(bundle: &CompiledSchemaBundle) -> Option<&coreconf_schema::SchemaModule> {
    bundle.modules.iter().find(|module| {
        let prefix = format!("/{}:", module.name);
        bundle.nodes.keys().any(|path| path.starts_with(&prefix))
    }).or_else(|| bundle.modules.first())
}

fn top_level_paths<'a>(bundle: &'a CompiledSchemaBundle, module_name: &str) -> Vec<&'a str> {
    let prefix = format!("/{}:", module_name);
    bundle
        .nodes
        .keys()
        .filter(|path| path.starts_with(&prefix) && path.trim_start_matches('/').split('/').count() == 1)
        .map(|path| path.as_str())
        .collect()
}

fn render_yang_node(out: &mut String, path: &str, bundle: &CompiledSchemaBundle, indent: usize) {
    let node = &bundle.nodes[path];
    let name = node_name(path);
    let spaces = "  ".repeat(indent);
    let keyword = match node.kind {
        coreconf_schema::NodeKind::Container => "container",
        coreconf_schema::NodeKind::List => "list",
        coreconf_schema::NodeKind::Leaf => "leaf",
        coreconf_schema::NodeKind::LeafList => "leaf-list",
        coreconf_schema::NodeKind::Rpc => "rpc",
        coreconf_schema::NodeKind::Input => "input",
        coreconf_schema::NodeKind::Output => "output",
    };

    out.push_str(&format!("{spaces}{keyword} {name} {{\n"));
    for must in &node.must {
        out.push_str(&format!("{spaces}  must \"{must}\";\n"));
    }
    if let Some(when) = &node.when {
        out.push_str(&format!("{spaces}  when \"{when}\";\n"));
    }
    for child in &node.children {
        render_yang_node(out, child, bundle, indent + 1);
    }
    out.push_str(&format!("{spaces}}}\n"));
}

fn node_name(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .and_then(|segment| segment.rsplit(':').next())
        .unwrap_or(path)
}
