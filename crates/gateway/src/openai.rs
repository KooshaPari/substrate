//! OpenAI-compatible response shapes used by the gateway.

use serde::Serialize;
use substrate_core::domain::RoutingDecision;

/// OpenAI model list response.
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    /// Always `"list"`.
    pub object: &'static str,
    /// Available models.
    pub data: Vec<ModelObject>,
}

/// OpenAI model object.
#[derive(Debug, Serialize)]
pub struct ModelObject {
    /// Model id.
    pub id: String,
    /// Always `"model"`.
    pub object: &'static str,
    /// Owner placeholder.
    pub owned_by: &'static str,
}

/// Build the OpenAI models list from a routing decision.
pub fn models_from_decision(decision: &RoutingDecision) -> ModelsResponse {
    ModelsResponse {
        object: "list",
        data: vec![ModelObject {
            id: decision.model.clone(),
            object: "model",
            owned_by: "substrate",
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use substrate_core::domain::RoutingDecision;

    #[test]
    fn models_from_decision_uses_model_id() {
        let decision = RoutingDecision {
            engine: "forge".into(),
            model: "test-model".into(),
            reason: Some("test".into()),
        };
        let response = models_from_decision(&decision);
        assert_eq!(response.object, "list");
        assert_eq!(response.data[0].id, "test-model");
    }
}
