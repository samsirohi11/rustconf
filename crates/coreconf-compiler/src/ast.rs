#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstModule {
    pub name: String,
    pub children: Vec<AstStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstStatement {
    pub keyword: String,
    pub argument: Option<String>,
    pub children: Vec<AstStatement>,
}
