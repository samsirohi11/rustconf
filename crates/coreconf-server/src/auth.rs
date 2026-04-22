#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub actor: String,
    pub action: String,
    pub resource: String,
}

pub trait Authorizer: Send + Sync {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct MemoryAuthorizer;

impl Authorizer for MemoryAuthorizer {
    fn authorize(&self, _request: &AuthorizationRequest) -> Result<(), String> {
        Ok(())
    }
}
