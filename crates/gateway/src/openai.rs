//! OpenAI-compatible request/response shapes and routing integration.

use serde::{Deserialize, Serialize};
use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::ports::RoutingPort;
use uuid::Uuid;

use crate::config::ProviderConfig;

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

/// Outcome of provider prefix resolution.
pub enum ProviderRoute {
    /// Model matched a known provider prefix; contains (provider, stripped_model).
    Provider(String, String),
    /// No prefix match — fall through to OmniRoute.
    OmniRoute,
}

/// Attempt to resolve a `provider/model` prefix from the registered providers.
///
/// Returns `ProviderRoute::Provider(provider_name, stripped_model)` on a match,
/// or `ProviderRoute::OmniRoute` when no prefix is found (backward-compat).
pub fn resolve_provider_route(providers: &[ProviderConfig], model: &str) -> ProviderRoute {
    if let Some(slash) = model.find('/') {
        let prefix = &model[..slash];
        let stripped = model[slash + 1..].to_string();
        if providers.iter().any(|p| p.name == prefix) {
            return ProviderRoute::Provider(prefix.to_string(), stripped);
        }
    }
    ProviderRoute::OmniRoute
}

/// Route a chat request and build an OpenAI-shaped completion response.
///
/// When the model field contains a `provider/model` prefix that matches a
/// registered provider, the request is forwarded to that provider and the
/// prefix is stripped before forwarding. Otherwise falls through to OmniRoute.
pub async fn complete_chat(
    routing: &dyn RoutingPort,
    req: &ChatCompletionRequest,
    providers: &[ProviderConfig],
) -> Result<ChatCompletionResponse, String> {
    let prompt = req.user_prompt()?;

    // Check if the model field carries a provider prefix
    match resolve_provider_route(providers, &req.model) {
        ProviderRoute::Provider(provider_name, stripped_model) => {
            // Find the provider config
            let provider = providers
                .iter()
                .find(|p| p.name == provider_name)
                .ok_or_else(|| format!("provider not found: {provider_name}"))?;

            // Resolve the API key at runtime (never hardcoded)
            let api_key = provider
                .resolve_api_key()
                .ok_or_else(|| format!("API key not available for provider {}", provider.name))?;

            // Forward to provider via HTTP
            forward_to_provider(provider, &stripped_model, req, &api_key).await
        }
        ProviderRoute::OmniRoute => {
            // Fall through to OmniRoute (existing behavior preserved)
            let task = Task::new(prompt, ".");
            let decision = routing
                .route_decision(&task)
                .await
                .map_err(|e| format!("routing failed: {e}"))?;
            Ok(chat_response_from_decision(&decision))
        }
    }
}

/// Forward the request to an OpenAI-compatible upstream provider and stream back the response.
async fn forward_to_provider(
    provider: &ProviderConfig,
    model: &str,
    req: &ChatCompletionRequest,
    api_key: &str,
) -> Result<ChatCompletionResponse, String> {
    let url = format!("{}/chat/completions", provider.base_url);

    // Build forwarded request body with the stripped model name
    let forward_body = serde_json::json!({
        "model": model,
        "messages": req.messages.iter().map(|m| {
            serde_json::json!({
                "role": match m.role {
                    ChatRole::System => "system",
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                },
                "content": m.content
            })
        }).collect::<Vec<_>>(),
    });

    let client = reqwest::Client::new();
    let http_resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "substrate-gateway")
        .json(&forward_body)
        .send()
        .await
        .map_err(|e| format!("upstream request to {} failed: {e}", provider.name))?;

    let status = http_resp.status();
    if !status.is_success() {
        let body = http_resp.text().await.unwrap_or_default();
        return Err(format!(
            "upstream provider {} returned {}: {}",
            provider.name, status, body
        ));
    }

    // Parse the upstream response as an OpenAI completion
    let upstream: serde_json::Value = http_resp
        .json()
        .await
        .map_err(|e| format!("failed to parse upstream response from {}: {e}", provider.name))?;

    // Extract content from the response
    let content = upstream["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let resp_model = upstream["model"]
        .as_str()
        .unwrap_or(model)
        .to_string();

    Ok(ChatCompletionResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion",
        model: resp_model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatChoiceMessage {
                role: "assistant",
                content,
            },
            finish_reason: "stop",
        }],
    })
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
    use crate::config::{builtin_providers, resolve_provider};

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

    #[test]
    fn test_provider_config_parsing() {
        let providers = builtin_providers();
        assert_eq!(providers.len(), 3);

        let opencode = providers.iter().find(|p| p.name == "opencode-go").unwrap();
        assert_eq!(opencode.base_url, "https://opencode.ai/zen/go/v1");
        assert_eq!(opencode.api_key_env, "OPENCODE_API_KEY");
        assert_eq!(
            opencode.default_model.as_deref(),
            Some("deepseek-v4-flash")
        );

        let deepseek = providers.iter().find(|p| p.name == "deepseek").unwrap();
        assert_eq!(deepseek.base_url, "https://api.deepseek.com/v1");
        assert_eq!(deepseek.api_key_env, "DEEPSEEK_API_KEY");

        let kilocode = providers.iter().find(|p| p.name == "kilocode").unwrap();
        assert_eq!(kilocode.base_url, "https://api.kilo.ai/api/gateway/v1");
        assert_eq!(kilocode.api_key_env, "KILOCODE_API_KEY");
    }

    #[test]
    fn test_model_prefix_routing() {
        let providers = builtin_providers();

        // deepseek/deepseek-chat should route to deepseek and strip prefix
        let result = resolve_provider(&providers, "deepseek/deepseek-chat");
        assert!(result.is_some(), "deepseek/deepseek-chat should match");
        let (provider, stripped) = result.unwrap();
        assert_eq!(provider.name, "deepseek");
        assert_eq!(stripped, "deepseek-chat");

        // kilocode/anthropic/claude-sonnet-4.5 — only first slash is the prefix
        let result2 = resolve_provider(&providers, "kilocode/anthropic/claude-sonnet-4.5");
        assert!(result2.is_some());
        let (p2, m2) = result2.unwrap();
        assert_eq!(p2.name, "kilocode");
        assert_eq!(m2, "anthropic/claude-sonnet-4.5");

        // plain model with no prefix → no match
        let result3 = resolve_provider(&providers, "gpt-4o");
        assert!(result3.is_none(), "plain model should not match any provider");

        // unknown prefix → no match
        let result4 = resolve_provider(&providers, "openai/gpt-4o");
        assert!(
            result4.is_none(),
            "unknown prefix should fall through to OmniRoute"
        );
    }

    #[test]
    fn resolve_provider_route_deepseek() {
        let providers = builtin_providers();
        match resolve_provider_route(&providers, "deepseek/deepseek-chat") {
            ProviderRoute::Provider(name, model) => {
                assert_eq!(name, "deepseek");
                assert_eq!(model, "deepseek-chat");
            }
            ProviderRoute::OmniRoute => panic!("expected provider route"),
        }
    }

    #[test]
    fn resolve_provider_route_omniroute_fallback() {
        let providers = builtin_providers();
        match resolve_provider_route(&providers, "accounts/fireworks/routers/kimi") {
            ProviderRoute::OmniRoute => {}
            ProviderRoute::Provider(n, _) => panic!("expected OmniRoute fallback, got {n}"),
        }
    }
}
