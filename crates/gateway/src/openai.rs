//! OpenAI-compatible request/response shapes and routing integration.

use bytes::Bytes;
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::ports::RoutingPort;
use uuid::Uuid;

use crate::config::ProviderConfig;
use crate::fallback::{try_with_fallback, FallbackChain};
use crate::retry::{with_retry, RetryPolicy, RetryableError};

/// OpenAI chat message role.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
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
    /// When `true`, the response is a stream of SSE chunks instead of a single JSON object.
    #[serde(default)]
    pub stream: bool,
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
            // Find the primary provider config
            let primary = providers
                .iter()
                .find(|p| p.name == provider_name)
                .ok_or_else(|| format!("provider not found: {provider_name}"))?;

            // Build a fallback chain if the provider declares fallbacks.
            if primary.fallbacks.is_empty() {
                // Fast path: no fallbacks configured — dispatch directly.
                let api_key = primary.resolve_api_key().ok_or_else(|| {
                    format!("API key not available for provider {}", primary.name)
                })?;
                forward_to_provider(primary, &stripped_model, req, &api_key).await
            } else {
                let chain = FallbackChain::from_provider_config(primary);
                let model = stripped_model.clone();
                let req_clone = req.clone();
                try_with_fallback(&chain, providers, |p: &ProviderConfig| {
                    let m = model.clone();
                    let r = req_clone.clone();
                    let key = p.resolve_api_key();
                    let p_clone = p.clone();
                    async move {
                        let api_key = key.ok_or_else(|| {
                            format!("API key not available for provider {}", p_clone.name)
                        })?;
                        forward_to_provider(&p_clone, &m, &r, &api_key).await
                    }
                })
                .await
            }
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
///
/// The HTTP call is wrapped with exponential back-off retry (policy from
/// [`RetryPolicy::default_policy`], overrideable via `SUBSTRATE_RETRY_ATTEMPTS`
/// and `SUBSTRATE_RETRY_BASE_MS` env vars).  5xx and 429 responses trigger a
/// retry; all other 4xx responses abort immediately.
async fn forward_to_provider(
    provider: &ProviderConfig,
    model: &str,
    req: &ChatCompletionRequest,
    api_key: &str,
) -> Result<ChatCompletionResponse, String> {
    let url = format!("{}/chat/completions", provider.base_url);
    let provider_name = provider.name.clone();

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

    let policy = RetryPolicy::default_policy();
    let client = reqwest::Client::new();

    let result = with_retry(&policy, || {
        let client = client.clone();
        let url = url.clone();
        let api_key = api_key.to_owned();
        let body = forward_body.clone();
        let pname = provider_name.clone();
        async move {
            let http_resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .header("Content-Type", "application/json")
                .header("User-Agent", "substrate-gateway")
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    RetryableError::retryable(format!("upstream request to {pname} failed: {e}"))
                })?;

            let status = http_resp.status();
            if !status.is_success() {
                let body_txt = http_resp.text().await.unwrap_or_default();
                return Err(RetryableError::from_status(
                    status.as_u16(),
                    &body_txt,
                    &pname,
                ));
            }

            // Parse the upstream response as an OpenAI completion.
            let upstream: serde_json::Value = http_resp.json().await.map_err(|e| {
                RetryableError::permanent(format!(
                    "failed to parse upstream response from {pname}: {e}"
                ))
            })?;

            Ok(upstream)
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    let content = result["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let resp_model = result["model"].as_str().unwrap_or(model).to_string();

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

// ---------------------------------------------------------------------------
// SSE streaming types and helpers
// ---------------------------------------------------------------------------

/// A content delta inside a streaming chunk choice.
#[derive(Debug, Clone, Serialize)]
pub struct StreamDelta {
    /// Role is only present in the first chunk (the "role" frame).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'static str>,
    /// Incremental text content.  Empty string in the final `finish_reason` chunk.
    pub content: String,
}

/// A single choice inside a `ChatCompletionChunk`.
#[derive(Debug, Clone, Serialize)]
pub struct StreamChoice {
    /// Choice index (always 0 for single-choice completions).
    pub index: u32,
    /// Incremental content delta.
    pub delta: StreamDelta,
    /// `null` until the final chunk, then `"stop"`.
    pub finish_reason: Option<&'static str>,
}

/// An OpenAI-format server-sent event chunk (`chat.completion.chunk`).
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionChunk {
    /// Chunk id (same across all chunks for one completion).
    pub id: String,
    /// Always `"chat.completion.chunk"`.
    pub object: &'static str,
    /// Model that produced this chunk.
    pub model: String,
    /// Choices carrying the content delta.
    pub choices: Vec<StreamChoice>,
}

/// Encode a [`ChatCompletionChunk`] as a single SSE `data:` frame.
///
/// Format: `data: <json>\n\n`
///
/// # Errors
/// Returns `Err` only when JSON serialisation fails — this should never happen
/// for our fixed-shape structs, but we surface it loudly rather than silently
/// dropping the frame.
pub fn encode_sse_chunk(chunk: &ChatCompletionChunk) -> Result<Bytes, String> {
    let json = serde_json::to_string(chunk)
        .map_err(|e| format!("SSE serialisation failed for chunk {}: {e}", chunk.id))?;
    Ok(Bytes::from(format!("data: {json}\n\n")))
}

/// The SSE stream terminator mandated by the OpenAI protocol.
pub fn encode_sse_done() -> Bytes {
    Bytes::from("data: [DONE]\n\n")
}

/// Build an SSE stream from a list of text tokens.
///
/// Emits:
/// 1. A "role" frame (delta.role = "assistant", delta.content = "").
/// 2. One frame per token in `tokens`.
/// 3. A "finish" frame (finish_reason = "stop", delta.content = "").
/// 4. The `data: [DONE]` terminator.
///
/// Errors surface as `Err` items in the stream — the stream does NOT silently
/// swallow them or fall back to a partial response.
pub fn token_stream(
    completion_id: String,
    model: String,
    tokens: Vec<String>,
) -> BoxStream<'static, Result<Bytes, std::io::Error>> {
    let role_chunk = ChatCompletionChunk {
        id: completion_id.clone(),
        object: "chat.completion.chunk",
        model: model.clone(),
        choices: vec![StreamChoice {
            index: 0,
            delta: StreamDelta {
                role: Some("assistant"),
                content: String::new(),
            },
            finish_reason: None,
        }],
    };

    let finish_chunk = ChatCompletionChunk {
        id: completion_id.clone(),
        object: "chat.completion.chunk",
        model: model.clone(),
        choices: vec![StreamChoice {
            index: 0,
            delta: StreamDelta {
                role: None,
                content: String::new(),
            },
            finish_reason: Some("stop"),
        }],
    };

    let token_chunks: Vec<ChatCompletionChunk> = tokens
        .into_iter()
        .map(|tok| ChatCompletionChunk {
            id: completion_id.clone(),
            object: "chat.completion.chunk",
            model: model.clone(),
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta {
                    role: None,
                    content: tok,
                },
                finish_reason: None,
            }],
        })
        .collect();

    // Assemble the ordered chunk sequence then append [DONE].
    let all_chunks: Vec<ChatCompletionChunk> = std::iter::once(role_chunk)
        .chain(token_chunks)
        .chain(std::iter::once(finish_chunk))
        .collect();

    let chunk_stream =
        stream::iter(all_chunks).map(move |c| encode_sse_chunk(&c).map_err(std::io::Error::other));

    let done_stream = stream::once(async { Ok::<Bytes, std::io::Error>(encode_sse_done()) });

    chunk_stream.chain(done_stream).boxed()
}

/// Route a chat request and return an SSE stream of `ChatCompletionChunk` frames.
///
/// The stream always ends with `data: [DONE]\n\n`.  Routing errors and upstream
/// errors are emitted as `Err` items — the caller (handler) MUST propagate them
/// to the client rather than silently dropping them.
pub async fn complete_chat_stream(
    routing: &dyn RoutingPort,
    req: &ChatCompletionRequest,
    providers: &[ProviderConfig],
) -> Result<BoxStream<'static, Result<Bytes, std::io::Error>>, String> {
    let prompt = req.user_prompt()?;

    match resolve_provider_route(providers, &req.model) {
        ProviderRoute::Provider(provider_name, stripped_model) => {
            let provider = providers
                .iter()
                .find(|p| p.name == provider_name)
                .ok_or_else(|| format!("provider not found: {provider_name}"))?;

            let api_key = provider
                .resolve_api_key()
                .ok_or_else(|| format!("API key not available for provider {}", provider.name))?;

            // Forward streaming request to the upstream provider.
            stream_from_provider(provider, &stripped_model, req, &api_key).await
        }
        ProviderRoute::OmniRoute => {
            // OmniRoute: get a routing decision and synthesise a single-chunk stream
            // (substrate itself is the "model" here — no upstream HTTP call).
            let task = Task::new(prompt, ".");
            let decision = routing
                .route_decision(&task)
                .await
                .map_err(|e| format!("routing failed: {e}"))?;

            let id = format!("chatcmpl-{}", Uuid::new_v4());
            let reason = decision.reason.as_deref().unwrap_or("routed");
            let content = format!(
                "routed to engine={} model={} ({reason})",
                decision.engine, decision.model
            );
            // Tokenise at word boundaries for a realistic multi-chunk stream.
            let tokens: Vec<String> = content
                .split_inclusive(' ')
                .map(|t| t.to_string())
                .collect();

            Ok(token_stream(id, decision.model, tokens))
        }
    }
}

/// Forward a streaming chat request to an OpenAI-compatible upstream and proxy
/// the raw SSE bytes back to the caller.
///
/// The initial HTTP connection attempt is wrapped with retry back-off
/// (same [`RetryPolicy::default_policy`] as the non-streaming path).
/// Once the connection is established, the raw byte stream is proxied
/// zero-copy; mid-stream errors surface as `Err` items in the stream.
///
/// # Errors
/// Returns `Err(String)` when the upstream HTTP request fails or returns a
/// non-2xx status.  Mid-stream upstream errors cause an `Err` item in the
/// returned stream — they are never silently swallowed.
async fn stream_from_provider(
    provider: &ProviderConfig,
    model: &str,
    req: &ChatCompletionRequest,
    api_key: &str,
) -> Result<BoxStream<'static, Result<Bytes, std::io::Error>>, String> {
    let url = format!("{}/chat/completions", provider.base_url);
    let provider_name = provider.name.clone();

    let forward_body = serde_json::json!({
        "model": model,
        "stream": true,
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

    let policy = RetryPolicy::default_policy();
    let client = reqwest::Client::new();

    let http_resp = with_retry(&policy, || {
        let client = client.clone();
        let url = url.clone();
        let api_key = api_key.to_owned();
        let body = forward_body.clone();
        let pname = provider_name.clone();
        async move {
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .header("Content-Type", "application/json")
                .header("Accept", "text/event-stream")
                .header("User-Agent", "substrate-gateway")
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    RetryableError::retryable(format!(
                        "upstream streaming request to {pname} failed: {e}"
                    ))
                })?;

            let status = resp.status();
            if !status.is_success() {
                let body_txt = resp.text().await.unwrap_or_default();
                return Err(RetryableError::from_status(
                    status.as_u16(),
                    &body_txt,
                    &pname,
                ));
            }
            Ok(resp)
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    // Pass the upstream SSE byte stream through directly.
    // `map_err` converts reqwest errors to io::Error so they appear as Err items
    // in the stream — they are never swallowed.
    use futures::TryStreamExt;
    let byte_stream = http_resp
        .bytes_stream()
        .map_ok(Bytes::from)
        .map_err(|e| std::io::Error::other(format!("upstream stream error: {e}")));

    Ok(byte_stream.boxed())
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
            stream: false,
        };
        assert_eq!(req.user_prompt().unwrap(), "hello");
    }

    #[test]
    fn user_prompt_rejects_empty_messages() {
        let req = ChatCompletionRequest {
            model: "auto".into(),
            messages: vec![],
            stream: false,
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
        assert_eq!(opencode.default_model.as_deref(), Some("deepseek-v4-flash"));

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
        assert!(
            result3.is_none(),
            "plain model should not match any provider"
        );

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

    // -----------------------------------------------------------------------
    // SSE encoding unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn encode_sse_chunk_produces_data_frame() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-test".into(),
            object: "chat.completion.chunk",
            model: "test-model".into(),
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta {
                    role: None,
                    content: "hello".into(),
                },
                finish_reason: None,
            }],
        };
        let bytes = encode_sse_chunk(&chunk).expect("serialisation must not fail");
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.starts_with("data: "), "must start with 'data: '");
        assert!(text.ends_with("\n\n"), "must end with double newline");
        assert!(
            text.contains("\"content\":\"hello\""),
            "must contain content"
        );
        assert!(
            text.contains("\"object\":\"chat.completion.chunk\""),
            "must carry object type"
        );
    }

    #[test]
    fn encode_sse_done_is_correct_terminator() {
        let bytes = encode_sse_done();
        assert_eq!(bytes, Bytes::from("data: [DONE]\n\n"));
    }

    #[tokio::test]
    async fn token_stream_happy_path_chunk_sequence() {
        use futures::StreamExt;
        let tokens = vec!["hello ".to_string(), "world".to_string()];
        let mut stream = token_stream("cmpl-1".into(), "m".into(), tokens);

        // Collect all frames
        let mut frames: Vec<String> = Vec::new();
        while let Some(result) = stream.next().await {
            let bytes = result.expect("stream must not error on happy path");
            frames.push(String::from_utf8(bytes.to_vec()).unwrap());
        }

        // Expect: role frame, 2 token frames, finish frame, [DONE]
        assert_eq!(
            frames.len(),
            5,
            "expected 5 frames (role + 2 tokens + finish + DONE)"
        );

        // First frame carries the role
        assert!(
            frames[0].contains("\"role\":\"assistant\""),
            "first frame must have role"
        );
        assert!(
            !frames[0].contains("\"finish_reason\":\"stop\""),
            "role frame must not carry finish_reason=stop"
        );

        // Token frames
        assert!(
            frames[1].contains("\"content\":\"hello \""),
            "second frame: 'hello '"
        );
        assert!(
            frames[2].contains("\"content\":\"world\""),
            "third frame: 'world'"
        );

        // Finish frame
        assert!(
            frames[3].contains("\"finish_reason\":\"stop\""),
            "finish frame must have finish_reason=stop"
        );

        // [DONE] terminator
        assert_eq!(frames[4], "data: [DONE]\n\n", "[DONE] must be exact");
    }

    #[tokio::test]
    async fn token_stream_empty_tokens_still_terminates() {
        use futures::StreamExt;
        let mut stream = token_stream("cmpl-empty".into(), "m".into(), vec![]);

        let mut frames: Vec<String> = Vec::new();
        while let Some(result) = stream.next().await {
            let bytes = result.expect("stream must not error");
            frames.push(String::from_utf8(bytes.to_vec()).unwrap());
        }

        // role + finish + [DONE] — 3 frames even with 0 tokens
        assert_eq!(frames.len(), 3, "empty token list: role + finish + DONE");
        assert_eq!(frames[2], "data: [DONE]\n\n");
    }

    #[tokio::test]
    async fn token_stream_done_always_last() {
        use futures::StreamExt;
        let tokens = vec!["a".into(), "b".into(), "c".into()];
        let mut stream = token_stream("cmpl-x".into(), "m".into(), tokens);

        let mut last = String::new();
        while let Some(result) = stream.next().await {
            let bytes = result.expect("no errors expected");
            last = String::from_utf8(bytes.to_vec()).unwrap();
        }

        assert_eq!(
            last, "data: [DONE]\n\n",
            "[DONE] must be the very last frame"
        );
    }

    #[test]
    fn stream_chunk_role_field_omitted_on_non_role_frames() {
        // StreamDelta with role=None must not serialise the `role` key
        let delta = StreamDelta {
            role: None,
            content: "tok".into(),
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(
            !json.contains("\"role\""),
            "role must be absent when None: {json}"
        );
    }

    #[test]
    fn stream_chunk_finish_reason_null_when_none() {
        let choice = StreamChoice {
            index: 0,
            delta: StreamDelta {
                role: None,
                content: "tok".into(),
            },
            finish_reason: None,
        };
        let json = serde_json::to_string(&choice).unwrap();
        // finish_reason: null is acceptable per OpenAI spec; just must not be "stop"
        assert!(
            !json.contains("\"finish_reason\":\"stop\""),
            "finish_reason must not be 'stop' on non-terminal choice: {json}"
        );
    }

    #[test]
    fn chat_completion_request_stream_defaults_to_false() {
        let json = r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hi"}]}"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(!req.stream, "stream should default to false when absent");
    }

    #[test]
    fn chat_completion_request_stream_true_deserialises() {
        let json =
            r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hi"}],"stream":true}"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.stream);
    }

    #[tokio::test]
    async fn complete_chat_stream_omniroute_produces_valid_sse() {
        use futures::StreamExt;
        use substrate_core::{
            domain::{RoutingDecision, Task},
            error::Result as SubstrateResult,
            ports::RoutingPort,
        };

        struct StubRouter;
        #[async_trait::async_trait]
        impl RoutingPort for StubRouter {
            async fn route_decision(&self, _task: &Task) -> SubstrateResult<RoutingDecision> {
                Ok(RoutingDecision {
                    engine: "test-engine".into(),
                    model: "test-model".into(),
                    reason: Some("unit-test".into()),
                })
            }
        }

        let req = ChatCompletionRequest {
            model: "auto".into(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "ping".into(),
            }],
            stream: true,
        };

        let mut stream = complete_chat_stream(&StubRouter, &req, &[])
            .await
            .expect("complete_chat_stream must not fail for OmniRoute");

        let mut all_frames: Vec<String> = Vec::new();
        while let Some(result) = stream.next().await {
            let bytes = result.expect("stream items must not error");
            all_frames.push(String::from_utf8(bytes.to_vec()).unwrap());
        }

        assert!(!all_frames.is_empty(), "must emit at least one frame");
        assert_eq!(
            all_frames.last().unwrap().as_str(),
            "data: [DONE]\n\n",
            "last frame must be [DONE]"
        );
        // Each non-DONE frame must be a valid SSE data frame
        for frame in all_frames.iter().filter(|f| *f != "data: [DONE]\n\n") {
            assert!(
                frame.starts_with("data: "),
                "frame must start with 'data: ': {frame}"
            );
            assert!(
                frame.ends_with("\n\n"),
                "frame must end with \\n\\n: {frame}"
            );
            // Inner JSON must be parseable
            let json_part = frame.trim_start_matches("data: ").trim_end_matches("\n\n");
            let _: serde_json::Value =
                serde_json::from_str(json_part).expect("frame JSON must be valid");
        }
    }
}
