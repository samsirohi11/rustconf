use crate::ast::{AstModule, AstModuleKind, AstStatement};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

pub fn parse_yin_module(input: &str) -> Result<AstModule, String> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<AstStatement> = Vec::new();
    let mut module_name = None;

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|err| err.to_string())?
        {
            Event::Start(start) => {
                let name = local_name(&start);
                let argument = attr_for_statement(&start, &name);
                if name == "module" || name == "submodule" {
                    module_name = argument.clone();
                }
                stack.push(AstStatement {
                    keyword: name,
                    argument,
                    children: Vec::new(),
                });
            }
            Event::Empty(start) => {
                let child = AstStatement {
                    keyword: local_name(&start),
                    argument: attr_for_statement(&start, &local_name(&start)),
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(child);
                }
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let child = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(child);
                } else {
                    break;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    let module = stack.pop().ok_or_else(|| "missing module element".to_string())?;
    Ok(AstModule {
        name: module_name.ok_or_else(|| "missing module name".to_string())?,
        kind: if module.keyword == "submodule" {
            AstModuleKind::Submodule
        } else {
            AstModuleKind::Module
        },
        belongs_to: module
            .children
            .iter()
            .find(|child| child.keyword == "belongs-to")
            .and_then(|child| child.argument.clone()),
        children: module.children,
    })
}

fn local_name(start: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(start.local_name().as_ref()).into_owned()
}

fn attr(start: &BytesStart<'_>, key: &str) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find(|attr| attr.key.as_ref() == key.as_bytes())
        .map(|attr| String::from_utf8_lossy(attr.value.as_ref()).into_owned())
}

fn attr_for_statement(start: &BytesStart<'_>, name: &str) -> Option<String> {
    match name {
        "module" | "submodule" | "container" | "list" | "leaf" | "leaf-list" | "type"
        | "grouping" | "uses" | "rpc" | "input" | "output" => attr(start, "name"),
        "namespace" => attr(start, "uri"),
        "prefix" => attr(start, "value"),
        "import" | "include" | "belongs-to" => attr(start, "module"),
        "must" | "when" => attr(start, "condition").or_else(|| attr(start, "value")),
        "augment" | "refine" => attr(start, "target-node"),
        _ => attr(start, "name").or_else(|| attr(start, "value")),
    }
}
