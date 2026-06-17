//! OpenAI-compatible request/response shapes and routing integration.

use serde::{Deserialize, Serialize};
use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::ports::RoutingPort;
use uuid::Uuid;

/// OpenAI chat message role.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// System prompt.
    System,
    /// User message.
    User,
    /// Assistant message.
    Assistant,
}

/// A single chat message in an OpenAI completion request.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessage {
    /// Message role.
    pub role: ChatRole,
    /// Message text content.
    pub content: String,
}

/// OpenAI chat completion request body.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionRequest {
    /// Requested model (may be overridden by routing).
    #[allow(dead_code)]
    pub model: String,
    /// Conversation messages.
    pub messages: Vec<ChatMessage>,
}

impl ChatCompletionRequest {
    /// Validate and extract the latest user prompt for routing.
    pub fn user_prompt(&self) -> Result<String, String> {
        if self.messages.is_empty() {
            return Err("messages must not be empty".to_string());
        }
        self.messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, ChatRole::User))
            .map(|m| m.content.clone())
            .filter(|p| !p.trim().is_empty())
            .ok_or_else(|| "messages must include a non-empty user message".to_string())
    }
}

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

/// OpenAI chat completion response.
#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    /// Completion id.
    pub id: String,
    /// Always `"chat.completion"`.
    pub object: &'static str,
    /// Routed model id.
    pub model: String,
    /// Completion choices.
    pub choices: Vec<ChatChoice>,
}

/// A single chat completion choice.
#[derive(Debug, Serialize)]
pub struct ChatChoice {
    /// Choice index.
    pub index: u32,
    /// Assistant message.
    pub message: ChatChoiceMessage,
    /// Finish reason.
    pub finish_reason: &'static str,
}

/// Assistant message in a completion choice.
#[derive(Debug, Serialize)]
pub struct ChatChoiceMessage {
    /// Always `"assistant"`.
    pub role: &'static str,
    /// Routed acknowledgement text.
    pub content: String,
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

/// Route a chat request and build an OpenAI-shaped completion response.
pub async fn complete_chat(
    routing: &dyn RoutingPort,
    req: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, String> {
    let prompt = req.user_prompt()?;
    let task = Task::new(prompt, ".");
    let decision = routing
        .route_decision(&task)
        .await
        .map_err(|e| format!("routing failed: {e}"))?;
    Ok(chat_response_from_decision(&decision))
}

fn chat_response_from_decision(decision: &RoutingDecision) -> ChatCompletionResponse {
    let reason = decision.reason.as_deref().unwrap_or("routed");
    ChatCompletionResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion",
        model: decision.model.clone(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatChoiceMessage {
                role: "assistant",
                content: format!(
                    "routed to engine={} model={} ({reason})",
                    decision.engine, decision.model
                ),
            },
            finish_reason: "stop",
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_prompt_extracts_latest_user_message() {
        let req = ChatCompletionRequest {
            model: "auto".into(),
            messages: vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: "sys".into(),
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: "hello".into(),
                },
            ],
        };
        assert_eq!(req.user_prompt().unwrap(), "hello");
    }

    #[test]
    fn user_prompt_rejects_empty_messages() {
        let req = ChatCompletionRequest {
            model: "auto".into(),
            messages: vec![],
        };
        assert!(req.user_prompt().is_err());
    }
}
