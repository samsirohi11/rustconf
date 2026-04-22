use crate::auth::{AuthorizationRequest, Authorizer};
use crate::operations::OperationRegistry;
use crate::store::Store;
use coreconf_schema::CompiledSchemaBundle;
use rust_coreconf::coap_types::{ContentFormat, Request, Response};
use rust_coreconf::{CoreconfModel, Datastore, RequestHandler};

pub struct CoreconfServer<S, A> {
    store: S,
    authorizer: A,
    operations: OperationRegistry,
    handler: Option<RequestHandler>,
}

impl<S, A> CoreconfServer<S, A>
where
    S: Store,
    A: Authorizer,
{
    pub fn new(store: S, authorizer: A, _audit: crate::NoopAuditSink) -> Self {
        Self {
            store,
            authorizer,
            operations: OperationRegistry::default(),
            handler: None,
        }
    }

    pub fn handle(&mut self, request: &Request) -> Response {
        let _ = &self.store;
        let _ = &self.operations;
        let _ = self.authorizer.authorize(&AuthorizationRequest {
            actor: "system".into(),
            action: format!("{:?}", request.method),
            resource: "/c".into(),
        });

        self.handler.as_mut().map(|handler| handler.handle(request)).unwrap_or_else(|| {
            Response::content(Vec::new(), ContentFormat::YangDataCbor)
        })
    }

    pub fn from_bundle(
        bundle: CompiledSchemaBundle,
        store: S,
        authorizer: A,
        _audit: crate::NoopAuditSink,
    ) -> Result<Self, String> {
        let model = CoreconfModel::from_bundle(bundle).map_err(|err| err.to_string())?;
        let datastore = Datastore::new(model);
        Ok(Self {
            store,
            authorizer,
            operations: OperationRegistry::default(),
            handler: Some(RequestHandler::new(datastore)),
        })
    }
}
