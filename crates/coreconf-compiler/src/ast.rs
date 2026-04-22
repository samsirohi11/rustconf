#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstModuleKind {
    Module,
    Submodule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstModule {
    pub name: String,
    pub kind: AstModuleKind,
    pub belongs_to: Option<String>,
    pub children: Vec<AstStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstStatement {
    pub keyword: String,
    pub argument: Option<String>,
    pub children: Vec<AstStatement>,
}
