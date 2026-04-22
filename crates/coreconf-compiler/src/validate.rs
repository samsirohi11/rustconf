use crate::ast::{AstModule, AstModuleKind, AstStatement};
use crate::repository::CompilerRepository;
use crate::xpath::validate_xpath as validate_xpath_impl;
use coreconf_schema::{
    CompiledSchemaBundle, IdentitySchema, NodeKind, OperationField, OperationSchema, ResolvedType,
    SchemaModule, SchemaNode, TypedefSchema, YangScalarType,
};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("invalid xpath: {0}")]
    InvalidXPath(String),
    #[error("missing import: {0}")]
    MissingImport(String),
    #[error("parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Default)]
struct ModuleContext {
    module_name: String,
    local_prefix: Option<String>,
    imports: HashMap<String, String>,
    typedefs: BTreeMap<String, YangScalarType>,
    identities: BTreeMap<String, Option<String>>,
    groupings: HashMap<String, Vec<AstStatement>>,
}

pub fn validate_xpath(input: &str) -> Result<(), ValidationError> {
    validate_xpath_impl(input).map_err(|err| ValidationError::InvalidXPath(err.to_string()))
}

pub fn compile_paths(paths: &[PathBuf]) -> Result<CompiledSchemaBundle, ValidationError> {
    let repo = CompilerRepository::load_paths(paths)
        .map_err(|err| ValidationError::MissingImport(err.to_string()))?;

    let parsed_modules: Vec<AstModule> = repo
        .iter()
        .map(|(_, source)| crate::parse_module(source).map_err(ValidationError::Parse))
        .collect::<Result<_, _>>()?;

    for module in &parsed_modules {
        for child in &module.children {
            if child.keyword == "import" {
                let import_name = child.argument.as_deref().unwrap_or_default();
                if repo.get(import_name).is_none() {
                    return Err(ValidationError::MissingImport(import_name.to_string()));
                }
            }
        }
    }

    let mut bundle = CompiledSchemaBundle {
        format_version: 1,
        modules: parsed_modules
            .iter()
            .filter(|module| matches!(module.kind, AstModuleKind::Module))
            .map(|module| SchemaModule {
                name: module.name.clone(),
                revision: "unknown".into(),
            })
            .collect(),
        typedefs: Vec::new(),
        identities: Vec::new(),
        nodes: BTreeMap::new(),
        operations: BTreeMap::new(),
    };

    for module in &parsed_modules {
        if !matches!(module.kind, AstModuleKind::Module) {
            continue;
        }

        let context = collect_context(module, &parsed_modules);
        bundle.typedefs.extend(context.typedefs.iter().map(|(name, base)| {
            let (module_name, typedef_name) = split_qualified_name(name);
            TypedefSchema {
                module: module_name.to_string(),
                name: typedef_name.to_string(),
                base: base.clone(),
            }
        }));
        bundle.identities.extend(context.identities.iter().map(|(name, base)| {
            let (module_name, identity_name) = split_qualified_name(name);
            IdentitySchema {
                module: module_name.to_string(),
                name: identity_name.to_string(),
                base: base.clone(),
            }
        }));

        let root = format!("/{}:", module.name);
        lower_statements(
            &module.children,
            &root,
            &context,
            &mut bundle.nodes,
            &mut bundle.operations,
        )?;
    }

    Ok(bundle)
}

fn collect_context(module: &AstModule, parsed_modules: &[AstModule]) -> ModuleContext {
    let mut context = ModuleContext {
        module_name: module.name.clone(),
        ..ModuleContext::default()
    };

    for source in parsed_modules.iter().filter(|candidate| {
        candidate.name == module.name || candidate.belongs_to.as_deref() == Some(module.name.as_str())
    }) {
        if source.name == module.name {
            for child in &source.children {
                match child.keyword.as_str() {
                    "prefix" => context.local_prefix = child.argument.clone(),
                    "import" => {
                        if let Some(module_name) = &child.argument {
                            if let Some(prefix) = child
                                .children
                                .iter()
                                .find(|entry| entry.keyword == "prefix")
                                .and_then(|entry| entry.argument.clone())
                            {
                                context.imports.insert(prefix, module_name.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let local_definition_context = ModuleContext {
        module_name: context.module_name.clone(),
        local_prefix: context.local_prefix.clone(),
        imports: context.imports.clone(),
        ..ModuleContext::default()
    };

    for source in parsed_modules.iter().filter(|candidate| {
        candidate.name == module.name || candidate.belongs_to.as_deref() == Some(module.name.as_str())
    }) {
        for child in &source.children {
            match child.keyword.as_str() {
                "grouping" => {
                    if let Some(name) = &child.argument {
                        context.groupings.insert(name.clone(), child.children.clone());
                    }
                }
                "typedef" | "identity" => extend_definition(
                    child,
                    &local_definition_context,
                    &mut context.typedefs,
                    &mut context.identities,
                ),
                _ => {}
            }
        }
    }

    let imported_modules = context.imports.values().cloned().collect::<Vec<_>>();
    for imported_module in imported_modules {
        extend_definitions_from_owner(&imported_module, parsed_modules, &mut context);
    }

    context
}

fn lower_statements(
    statements: &[AstStatement],
    parent_path: &str,
    context: &ModuleContext,
    nodes: &mut BTreeMap<String, SchemaNode>,
    operations: &mut BTreeMap<String, OperationSchema>,
) -> Result<Vec<String>, ValidationError> {
    let mut lowered = Vec::new();

    for statement in statements {
        match statement.keyword.as_str() {
            "namespace" | "prefix" | "typedef" | "grouping" | "import" | "include" => {}
            "uses" => {
                if let Some(name) = &statement.argument {
                    if let Some(group_children) = context.groupings.get(name) {
                        lowered.extend(lower_statements(
                            group_children,
                            parent_path,
                            context,
                            nodes,
                            operations,
                        )?);
                    }
                }
            }
            "container" | "list" | "leaf" | "leaf-list" => {
                let argument = statement.argument.clone().unwrap_or_default();
                let path = join_path(parent_path, &argument);
                let kind = match statement.keyword.as_str() {
                    "container" => NodeKind::Container,
                    "list" => NodeKind::List,
                    "leaf" => NodeKind::Leaf,
                    "leaf-list" => NodeKind::LeafList,
                    _ => unreachable!(),
                };
                let must = statement
                    .children
                    .iter()
                    .filter(|child| child.keyword == "must")
                    .filter_map(|child| child.argument.clone())
                    .collect::<Vec<_>>();
                for expr in &must {
                    validate_xpath(expr)?;
                }
                let when = statement
                    .children
                    .iter()
                    .find(|child| child.keyword == "when")
                    .and_then(|child| child.argument.clone());
                if let Some(expr) = &when {
                    validate_xpath(expr)?;
                }
                let keys = statement
                    .children
                    .iter()
                    .find(|child| child.keyword == "key")
                    .and_then(|child| child.argument.clone())
                    .map(|keys| {
                        keys.split_whitespace()
                            .map(|key| join_path(&path, key))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let children = lower_statements(&statement.children, &path, context, nodes, operations)?;
                let type_stmt = statement.children.iter().find(|child| child.keyword == "type");
                let yang_type = type_stmt.and_then(|child| child.argument.as_deref()).map(map_type);
                let type_ref = type_stmt.and_then(|child| resolve_type_ref(child, &path, context));

                nodes.insert(
                    path.clone(),
                    SchemaNode {
                        path: path.clone(),
                        sid: None,
                        kind,
                        yang_type,
                        type_ref,
                        keys,
                        children: children.clone(),
                        must,
                        when,
                    },
                );
                lowered.push(path);
            }
            "rpc" => {
                let path = join_path(parent_path, statement.argument.as_deref().unwrap_or_default());
                let input = statement
                    .children
                    .iter()
                    .find(|child| child.keyword == "input")
                    .map(lower_operation_fields)
                    .unwrap_or_default();
                let output = statement
                    .children
                    .iter()
                    .find(|child| child.keyword == "output")
                    .map(lower_operation_fields)
                    .unwrap_or_default();
                operations.insert(
                    path.clone(),
                    OperationSchema {
                        path,
                        input,
                        output,
                    },
                );
            }
            _ => {}
        }
    }

    Ok(lowered)
}

fn lower_operation_fields(statement: &AstStatement) -> Vec<OperationField> {
    statement
        .children
        .iter()
        .filter(|child| child.keyword == "leaf")
        .map(|child| OperationField {
            name: child.argument.clone().unwrap_or_default(),
            yang_type: child
                .children
                .iter()
                .find(|sub| sub.keyword == "type")
                .and_then(|sub| sub.argument.as_deref())
                .map(map_type),
        })
        .collect()
}

fn resolve_type_ref(type_stmt: &AstStatement, current_path: &str, context: &ModuleContext) -> Option<ResolvedType> {
    let arg = type_stmt.argument.as_deref()?;

    if is_builtin_type(arg) {
        if arg == "identityref" {
            let base = type_stmt
                .children
                .iter()
                .find(|child| child.keyword == "base")
                .and_then(|child| child.argument.as_deref())
                .map(|entry| qualify_identifier(entry, context))?;
            let allowed = context
                .identities
                .iter()
                .filter_map(|(name, parent)| {
                    (parent.as_deref() == Some(base.as_str())).then(|| name.clone())
                })
                .collect::<Vec<_>>();
            return Some(ResolvedType::IdentityRef { base, allowed });
        }

        if arg == "leafref" {
            let target_path = type_stmt
                .children
                .iter()
                .find(|child| child.keyword == "path")
                .and_then(|child| child.argument.as_deref())
                .map(|path| normalize_leafref_path(current_path, path))?;
            return Some(ResolvedType::LeafRef { target_path });
        }

        return None;
    }

    let qualified = qualify_identifier(arg, context);
    context.typedefs.get(&qualified).map(|base| ResolvedType::Typedef {
        name: qualified,
        base: base.clone(),
    })
}

fn qualify_identifier(input: &str, context: &ModuleContext) -> String {
    if let Some((prefix, name)) = input.split_once(':') {
        if context.local_prefix.as_deref() == Some(prefix) {
            return format!("{}:{name}", context.module_name);
        }
        if let Some(module_name) = context.imports.get(prefix) {
            return format!("{module_name}:{name}");
        }
        return input.to_string();
    }

    format!("{}:{input}", context.module_name)
}

fn extend_definitions_from_owner(
    owner_name: &str,
    parsed_modules: &[AstModule],
    target: &mut ModuleContext,
) {
    let Some(owner) = parsed_modules
        .iter()
        .find(|candidate| candidate.name == owner_name && matches!(candidate.kind, AstModuleKind::Module))
    else {
        return;
    };

    let owner_context = ModuleContext {
        module_name: owner.name.clone(),
        local_prefix: owner
            .children
            .iter()
            .find(|child| child.keyword == "prefix")
            .and_then(|child| child.argument.clone()),
        imports: owner
            .children
            .iter()
            .filter(|child| child.keyword == "import")
            .filter_map(|child| {
                let module_name = child.argument.clone()?;
                let prefix = child
                    .children
                    .iter()
                    .find(|entry| entry.keyword == "prefix")
                    .and_then(|entry| entry.argument.clone())?;
                Some((prefix, module_name))
            })
            .collect(),
        ..ModuleContext::default()
    };

    for source in parsed_modules.iter().filter(|candidate| {
        candidate.name == owner.name || candidate.belongs_to.as_deref() == Some(owner.name.as_str())
    }) {
        for child in &source.children {
            if matches!(child.keyword.as_str(), "typedef" | "identity") {
                extend_definition(child, &owner_context, &mut target.typedefs, &mut target.identities);
            }
        }
    }
}

fn extend_definition(
    statement: &AstStatement,
    context: &ModuleContext,
    typedefs: &mut BTreeMap<String, YangScalarType>,
    identities: &mut BTreeMap<String, Option<String>>,
) {
    match statement.keyword.as_str() {
        "typedef" => {
            if let Some(name) = &statement.argument {
                let base = statement
                    .children
                    .iter()
                    .find(|entry| entry.keyword == "type")
                    .and_then(|entry| entry.argument.as_deref())
                    .map(map_type)
                    .unwrap_or(YangScalarType::String);
                let qualified = qualify_identifier(name, context);
                typedefs.insert(qualified, base);
            }
        }
        "identity" => {
            if let Some(name) = &statement.argument {
                let qualified = qualify_identifier(name, context);
                let base = statement
                    .children
                    .iter()
                    .find(|entry| entry.keyword == "base")
                    .and_then(|entry| entry.argument.as_deref())
                    .map(|entry| qualify_identifier(entry, context));
                identities.insert(qualified, base);
            }
        }
        _ => {}
    }
}

fn split_qualified_name(input: &str) -> (&str, &str) {
    input.split_once(':').unwrap_or(("unknown", input))
}

fn normalize_leafref_path(current_path: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.to_string();
    }

    let mut segments = current_path
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();

    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if segments.len() > 1 {
                    segments.pop();
                }
            }
            value => segments.push(value.to_string()),
        }
    }

    if segments.first().map(|entry| entry.is_empty()).unwrap_or(false) {
        format!("/{}", segments[1..].join("/"))
    } else {
        segments.join("/")
    }
}

fn join_path(parent_path: &str, name: &str) -> String {
    if parent_path.ends_with(':') {
        format!("{}{}", parent_path, name)
    } else {
        format!("{}/{}", parent_path, name)
    }
}

fn is_builtin_type(input: &str) -> bool {
    matches!(input, "string" | "boolean" | "int64" | "uint64" | "identityref" | "leafref")
}

fn map_type(input: &str) -> YangScalarType {
    match input {
        "boolean" => YangScalarType::Boolean,
        "int64" => YangScalarType::Int64,
        "uint64" => YangScalarType::Uint64,
        "identityref" => YangScalarType::IdentityRef,
        "leafref" => YangScalarType::LeafRef,
        _ => YangScalarType::String,
    }
}
