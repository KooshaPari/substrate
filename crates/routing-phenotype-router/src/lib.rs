//! RoutingPort adapter backed by phenotype-router's decision layer.
//!
//! This adapter keeps substrate's core free of adapter dependencies while
//! delegating the live routing decision to `phenotype-router`. The adapter
//! boundary owns request serialization, response mapping, and error
//! translation into `substrate_core::error::SubstrateError`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use phenotype_router::{BifrostAdapter, Decision, DecisionLayer, Request, Response};
use serde::Serialize;
use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::RoutingPort;

/// Default engine selected by the adapter when phenotype-router allows a task.
pub const DEFAULT_ENGINE: &str = "forge";

/// Default model selected by the adapter when phenotype-router allows a task.
pub const DEFAULT_MODEL: &str = "accounts/fireworks/routers/kimi-k2p6-turbo";

/// Adapter that delegates to phenotype-router and maps the decision into
/// substrate's `RoutingDecision` shape.
#[derive(Clone)]
pub struct PhenotypeRouterAdapter {
    decision_layer: Arc<dyn DecisionLayer>,
    engine: String,
    model: String,
}

impl std::fmt::Debug for PhenotypeRouterAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhenotypeRouterAdapter")
            .field("decision_layer", &"<dyn DecisionLayer>")
            .field("engine", &self.engine)
            .field("model", &self.model)
            .finish()
    }
}

impl Default for PhenotypeRouterAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PhenotypeRouterAdapter {
    /// Construct the adapter with phenotype-router's built-in Bifrost decision
    /// layer and substrate's default forge/kimi target.
    pub fn new() -> Self {
        Self {
            decision_layer: Arc::new(BifrostAdapter::new()),
            engine: DEFAULT_ENGINE.to_string(),
            model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Construct the adapter around a custom phenotype-router decision layer.
    pub fn with_decision_layer(decision_layer: impl DecisionLayer + 'static) -> Self {
        Self {
            decision_layer: Arc::new(decision_layer),
            ..Self::new()
        }
    }

    /// Override the substrate routing target returned by this adapter.
    pub fn with_target(mut self, engine: impl Into<String>, model: impl Into<String>) -> Self {
        self.engine = engine.into();
        self.model = model.into();
        self
    }

    fn build_request<T: Serialize>(request_id: String, body: &T) -> Result<Request> {
        let payload = serde_json::to_string(body).map_err(|e| {
            SubstrateError::Routing(format!(
                "routing-phenotype-router: failed to serialize request {request_id}: {e}"
            ))
        })?;
        Ok(Request::new(request_id, payload))
    }

    fn build_task_request(task: &Task) -> Result<Request> {
        Self::build_request(task.id.to_string(), task)
    }

    fn trace_summary(response: &Response) -> Option<String> {
        if response.trace.is_empty() {
            return None;
        }
        let trace = response
            .trace
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        Some(trace)
    }

    fn decision_reason(response: &Response) -> String {
        let mut reason = match &response.decision {
            Decision::Allow => "phenotype-router:allow".to_string(),
            Decision::Defer => "phenotype-router:defer".to_string(),
            Decision::Deny(msg) => format!("phenotype-router:deny:{msg}"),
        };
        if let Some(trace) = Self::trace_summary(response) {
            reason.push_str("; trace=");
            reason.push_str(&trace);
        }
        reason
    }

    fn map_response(&self, response: &Response) -> RoutingDecision {
        let mut decision = RoutingDecision::default_forge_kimi();
        decision.engine = self.engine.clone();
        decision.model = self.model.clone();
        decision.reason = Some(Self::decision_reason(response));
        decision
    }

    fn decide_inner(&self, task: &Task) -> Result<RoutingDecision> {
        let request = Self::build_task_request(task)?;
        let response = self.decision_layer.decide(&request);
        Ok(self.map_response(&response))
    }
}

#[async_trait]
impl RoutingPort for PhenotypeRouterAdapter {
    async fn route_decision(&self, task: &Task) -> Result<RoutingDecision> {
        self.decide_inner(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    struct StaticDecisionLayer {
        response: Response,
        seen: std::sync::Arc<Mutex<Vec<Request>>>,
    }

    impl StaticDecisionLayer {
        fn new(response: Response) -> (Self, std::sync::Arc<Mutex<Vec<Request>>>) {
            let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    response,
                    seen: seen.clone(),
                },
                seen,
            )
        }
    }

    impl DecisionLayer for StaticDecisionLayer {
        fn name(&self) -> &str {
            "static"
        }

        fn decide(&self, req: &Request) -> Response {
            self.seen.lock().unwrap().push(req.clone());
            self.response.clone()
        }
    }

    #[derive(Debug)]
    struct FailingSerializeTask;

    impl Serialize for FailingSerializeTask {
        fn serialize<S>(&self, _serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("boom"))
        }
    }

    #[test]
    fn build_request_serializes_full_task_shape() {
        let task = Task::new("route this", "/tmp/work");
        let request = PhenotypeRouterAdapter::build_task_request(&task).unwrap();
        assert_eq!(request.id, task.id.to_string());
        assert!(request.payload.contains("\"prompt\":\"route this\""));
        assert!(request.payload.contains("\"cwd\":\"/tmp/work\""));
    }

    #[tokio::test]
    async fn routes_allow_responses_into_default_routing_decision() {
        let (layer, seen) = StaticDecisionLayer::new(Response {
            decision: Decision::Allow,
            trace: vec![("router.adapter".to_string(), "static".to_string())],
        });
        let adapter = PhenotypeRouterAdapter::with_decision_layer(layer);
        let task = Task::new("anything", "/tmp");

        let decision = adapter.route_decision(&task).await.unwrap();

        assert_eq!(decision.engine, DEFAULT_ENGINE);
        assert_eq!(decision.model, DEFAULT_MODEL);
        assert_eq!(
            decision.reason.as_deref(),
            Some("phenotype-router:allow; trace=router.adapter=static")
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].id, task.id.to_string());
        assert!(seen[0].payload.contains("\"prompt\":\"anything\""));
        assert!(seen[0].payload.contains("\"cwd\":\"/tmp\""));
    }

    #[tokio::test]
    async fn routes_deny_responses_into_routing_reason() {
        let (layer, seen) = StaticDecisionLayer::new(Response {
            decision: Decision::Deny("blocked".to_string()),
            trace: vec![("router.id".to_string(), "deny-case".to_string())],
        });
        let adapter =
            PhenotypeRouterAdapter::with_decision_layer(layer).with_target("inert", "model-z");
        let task = Task::new("anything", "/tmp");

        let decision = adapter.route_decision(&task).await.unwrap();

        assert_eq!(decision.engine, "inert");
        assert_eq!(decision.model, "model-z");
        assert_eq!(
            decision.reason.as_deref(),
            Some("phenotype-router:deny:blocked; trace=router.id=deny-case")
        );
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn with_decision_layer_keeps_default_target() {
        let (layer, _) = StaticDecisionLayer::new(Response::allow());
        let adapter = PhenotypeRouterAdapter::with_decision_layer(layer);
        assert_eq!(adapter.engine, DEFAULT_ENGINE);
        assert_eq!(adapter.model, DEFAULT_MODEL);
    }

    #[test]
    fn serialize_failure_maps_at_adapter_boundary() {
        let err =
            PhenotypeRouterAdapter::build_request("task-1".to_string(), &FailingSerializeTask)
                .unwrap_err();
        match err {
            SubstrateError::Routing(msg) => {
                assert!(msg.contains("routing-phenotype-router"));
                assert!(msg.contains("task-1"));
            }
            other => panic!("expected routing error, got {other:?}"),
        }
    }
}
