use coreconf_server::{AuthorizationRequest, Authorizer, StaticTokenAuthorizer};

#[test]
fn static_token_authorizer_rejects_wrong_token() {
    let authorizer = StaticTokenAuthorizer::new([("cli-admin", "secret-token")]);

    let result = authorizer.authorize(&AuthorizationRequest {
        actor: "cli-admin".into(),
        action: "iPATCH".into(),
        resource: "/c".into(),
        token: Some("wrong-token".into()),
    });

    assert!(result.is_err());
}
