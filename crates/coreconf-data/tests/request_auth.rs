use rust_coreconf::coap_types::{Method, Request};

#[test]
fn request_carries_actor_and_token() {
    let request = Request::new(Method::Get)
        .with_actor("cli-admin")
        .with_auth_token("secret-token");

    assert_eq!(request.actor.as_deref(), Some("cli-admin"));
    assert_eq!(request.auth_token.as_deref(), Some("secret-token"));
}
