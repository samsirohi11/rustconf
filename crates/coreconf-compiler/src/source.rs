use crate::ast::AstModule;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Yang,
    Yin,
}

pub fn detect_format(path: &Path, source: &str) -> SourceFormat {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("yin") => SourceFormat::Yin,
        Some("yang") => SourceFormat::Yang,
        _ if source.trim_start().starts_with('<') => SourceFormat::Yin,
        _ => SourceFormat::Yang,
    }
}

pub fn parse_source(path: &Path, source: &str) -> Result<AstModule, String> {
    match detect_format(path, source) {
        SourceFormat::Yang => crate::parser::parse_module(source),
        SourceFormat::Yin => crate::yin::parse_yin_module(source),
    }
}
