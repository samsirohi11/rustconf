use crate::audit::{AuditEvent, AuditSink};
use crate::auth::{AuthorizationRequest, Authorizer};
use crate::operations::OperationRegistry;
use crate::store::Store;
use coreconf_schema::CompiledSchemaBundle;
use rust_coreconf::coap_types::{ContentFormat, Method, Request, Response, ResponseCode};
use rust_coreconf::instance_id::{decode_instances, encode_instances, Instance, InstancePath};
use rust_coreconf::{CoreconfModel, Datastore, RequestHandler};
use serde_json::Value;

pub struct CoreconfServer<S, A, Au = crate::NoopAuditSink> {
    store: S,
    authorizer: A,
    audit: Au,
    operations: OperationRegistry,
    schema_version: Option<String>,
    handler: Option<RequestHandler>,
}

impl<S, A, Au> CoreconfServer<S, A, Au>
where
    S: Store,
    A: Authorizer,
    Au: AuditSink,
{
    pub fn new(store: S, authorizer: A, audit: Au) -> Self {
        Self {
            store,
            authorizer,
            audit,
            operations: OperationRegistry::default(),
            schema_version: None,
            handler: None,
        }
    }

    pub fn operations_mut(&mut self) -> &mut OperationRegistry {
        &mut self.operations
    }

    pub fn handle(&mut self, request: &Request) -> Response {
        let actor = request.actor.clone().unwrap_or_else(|| "anonymous".into());
        let auth = AuthorizationRequest {
            actor: actor.clone(),
            action: request.method.to_string(),
            resource: "/c".into(),
            token: request.auth_token.clone(),
        };

        if let Err(err) = self.authorizer.authorize(&auth) {
            return Response::error(ResponseCode::Unauthorized, &err);
        }

        match request.method {
            Method::Post => self.handle_post(request, &actor),
            Method::IPatch => {
                let response = self
                    .handler
                    .as_mut()
                    .map(|handler| handler.handle(request))
                    .unwrap_or_else(|| Response::content(Vec::new(), ContentFormat::YangDataCbor));
                if response.code.is_success() {
                    if let Err(err) = self.persist_snapshot() {
                        return Response::error(ResponseCode::InternalServerError, &err);
                    }
                    if let Err(err) = self.record_audit(&actor, "write", "/c") {
                        return Response::error(ResponseCode::InternalServerError, &err);
                    }
                }
                response
            }
            _ => self
                .handler
                .as_mut()
                .map(|handler| handler.handle(request))
                .unwrap_or_else(|| Response::content(Vec::new(), ContentFormat::YangDataCbor)),
        }
    }

    pub fn from_bundle(
        bundle: CompiledSchemaBundle,
        mut store: S,
        authorizer: A,
        audit: Au,
    ) -> Result<Self, String> {
        let schema_version = schema_version(&bundle);
        store.write_bundle(&schema_version, &bundle)?;
        store.set_active_schema_version(&schema_version)?;

        let model = CoreconfModel::from_bundle(bundle.clone()).map_err(|err| err.to_string())?;
        let datastore = match store.read_snapshot(&schema_version)? {
            Some(snapshot) => Datastore::with_data(model, snapshot),
            None => Datastore::new(model),
        };

        Ok(Self {
            store,
            authorizer,
            audit,
            operations: OperationRegistry::from_bundle(&bundle),
            schema_version: Some(schema_version),
            handler: Some(RequestHandler::new(datastore)),
        })
    }

    fn handle_post(&mut self, request: &Request, actor: &str) -> Response {
        if let Some(format) = request.content_format
            && format != ContentFormat::YangInstancesCborSeq
        {
            return Response::error(
                ResponseCode::UnsupportedContentFormat,
                "expected yang-instances+cbor-seq",
            );
        }

        let Some(handler) = self.handler.as_mut() else {
            return Response::error(ResponseCode::InternalServerError, "server not initialized");
        };

        let instances = match decode_instances(&request.payload) {
            Ok(instances) => instances,
            Err(err) => return Response::error(ResponseCode::BadRequest, &err.to_string()),
        };

        let mut outputs = Vec::with_capacity(instances.len());
        for instance in instances {
            let Some(sid) = instance.path.absolute_sid() else {
                return Response::error(ResponseCode::BadRequest, "missing operation sid");
            };
            let Some(path) = handler.datastore().model().sid_file.get_identifier(sid) else {
                return Response::not_found(&format!("operation sid {}", sid));
            };

            let output = match self
                .operations
                .invoke(path, instance.value.unwrap_or(Value::Null))
            {
                Ok(output) => output,
                Err(_) => return Response::not_found(path),
            };

            let mut result_path = InstancePath::new();
            result_path.push_delta(sid);
            outputs.push(Instance::new(result_path, output));
        }

        if let Err(err) = self.record_audit(actor, "post", "/c") {
            return Response::error(ResponseCode::InternalServerError, &err);
        }

        match encode_instances(&outputs) {
            Ok(payload) => Response {
                code: ResponseCode::Changed,
                payload,
                content_format: Some(ContentFormat::YangInstancesCborSeq),
            },
            Err(err) => Response::error(ResponseCode::InternalServerError, &err.to_string()),
        }
    }

    fn persist_snapshot(&mut self) -> Result<(), String> {
        let Some(schema_version) = &self.schema_version else {
            return Ok(());
        };
        let Some(handler) = self.handler.as_ref() else {
            return Ok(());
        };
        self.store
            .write_snapshot(schema_version, handler.datastore().get_all())
    }

    fn record_audit(&mut self, actor: &str, action: &str, resource: &str) -> Result<(), String> {
        let event = AuditEvent::new(actor, action, resource);
        self.store.append_audit(event.clone())?;
        self.audit.record(&event)
    }
}

fn schema_version(bundle: &CompiledSchemaBundle) -> String {
    let module = bundle.modules.first();
    format!(
        "{}@{}",
        module.map(|entry| entry.name.as_str()).unwrap_or("unknown"),
        module.map(|entry| entry.revision.as_str()).unwrap_or("unknown")
    )
}
