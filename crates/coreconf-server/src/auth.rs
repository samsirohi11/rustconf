use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub actor: String,
    pub action: String,
    pub resource: String,
    pub token: Option<String>,
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

#[derive(Debug, Default)]
pub struct StaticTokenAuthorizer {
    tokens: BTreeMap<String, String>,
}

impl StaticTokenAuthorizer {
    pub fn new<I, A, T>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (A, T)>,
        A: Into<String>,
        T: Into<String>,
    {
        Self {
            tokens: pairs
                .into_iter()
                .map(|(actor, token)| (actor.into(), token.into()))
                .collect(),
        }
    }
}

impl Authorizer for StaticTokenAuthorizer {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), String> {
        let expected = self
            .tokens
            .get(&request.actor)
            .ok_or_else(|| format!("unknown actor: {}", request.actor))?;
        match request.token.as_deref() {
            Some(token) if token == expected => Ok(()),
            _ => Err("invalid auth token".into()),
        }
    }
}
