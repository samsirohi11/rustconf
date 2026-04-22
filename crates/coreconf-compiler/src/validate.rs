use crate::ast::{AstModule, AstStatement};
use crate::repository::CompilerRepository;
use crate::xpath::validate_xpath as validate_xpath_impl;
use coreconf_schema::{
    CompiledSchemaBundle, NodeKind, OperationField, OperationSchema, SchemaModule, SchemaNode,
    YangScalarType,
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
        let groups = collect_groupings(module);
        let root = format!("/{}:", module.name);
        lower_statements(
            &module.children,
            &root,
            &groups,
            &mut bundle.nodes,
            &mut bundle.operations,
        )?;
    }

    Ok(bundle)
}

fn collect_groupings(module: &AstModule) -> HashMap<String, Vec<AstStatement>> {
    let mut groups = HashMap::new();
    for child in &module.children {
        if child.keyword == "grouping" {
            if let Some(name) = &child.argument {
                groups.insert(name.clone(), child.children.clone());
            }
        }
    }
    groups
}

fn lower_statements(
    statements: &[AstStatement],
    parent_path: &str,
    groups: &HashMap<String, Vec<AstStatement>>,
    nodes: &mut BTreeMap<String, SchemaNode>,
    operations: &mut BTreeMap<String, OperationSchema>,
) -> Result<Vec<String>, ValidationError> {
    let mut lowered = Vec::new();

    for statement in statements {
        match statement.keyword.as_str() {
            "namespace" | "prefix" | "typedef" | "grouping" | "import" => {}
            "uses" => {
                if let Some(name) = &statement.argument {
                    if let Some(group_children) = groups.get(name) {
                        lowered.extend(lower_statements(
                            group_children,
                            parent_path,
                            groups,
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
                let children = lower_statements(&statement.children, &path, groups, nodes, operations)?;
                let yang_type = statement
                    .children
                    .iter()
                    .find(|child| child.keyword == "type")
                    .and_then(|child| child.argument.as_deref())
                    .map(map_type);

                nodes.insert(
                    path.clone(),
                    SchemaNode {
                        path: path.clone(),
                        sid: None,
                        kind,
                        yang_type,
                        type_ref: None,
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

fn join_path(parent_path: &str, name: &str) -> String {
    if parent_path.ends_with(':') {
        format!("{}{}", parent_path, name)
    } else {
        format!("{}/{}", parent_path, name)
    }
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
