use std::sync::Arc;

use dynaql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute},
    parser::types::OperationDefinition,
    Response,
};
use grafbase_runtime::{
    log::{LogEventReceiver, LogEventType, OperationType},
    GraphqlRequestExecutionContext,
};

pub struct RuntimeLogExtension {
    log_event_receiver: Arc<Box<dyn LogEventReceiver + Send + Sync>>,
}

impl RuntimeLogExtension {
    pub fn new(receiver: Box<dyn LogEventReceiver + Send + Sync>) -> Self {
        Self {
            log_event_receiver: Arc::new(receiver),
        }
    }
}

impl ExtensionFactory for RuntimeLogExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(Self {
            log_event_receiver: self.log_event_receiver.clone(),
        })
    }
}

#[async_trait::async_trait]
impl Extension for RuntimeLogExtension {
    /// Called at execute query.
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        operation: &OperationDefinition,
        next: NextExecute<'_>,
    ) -> Response {
        use dynaql::parser::types::OperationType as ParserOperationType;

        let request_id = &ctx
            .data::<GraphqlRequestExecutionContext>()
            .expect("must be set")
            .ray_id;

        self.log_event_receiver
            .invoke(request_id, LogEventType::OperationStarted { name: operation_name })
            .await;

        let start = wasm_timer::SystemTime::now();

        let response = next.run(ctx, operation_name, operation).await;

        let end = wasm_timer::SystemTime::now();

        let duration = end.duration_since(start).unwrap();
        self.log_event_receiver
            .invoke(
                request_id,
                LogEventType::OperationCompleted {
                    name: operation_name,
                    duration,
                    r#type: match response.operation_type {
                        ParserOperationType::Query => OperationType::Query {
                            is_introspection: crate::is_operation_introspection(operation),
                        },
                        ParserOperationType::Mutation => OperationType::Mutation,
                        ParserOperationType::Subscription => OperationType::Subscription,
                    },
                },
            )
            .await;

        response
    }
}
