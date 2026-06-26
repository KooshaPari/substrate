//! Kilo LLM gateway client.

use reqwest::Client;
use serde::Deserialize;
use substrate_core::error::{Result, SubstrateError};

/// Default Kilo gateway base URL (LLM chat completions only).
pub const DEFAULT_GATEWAY_URL: &str = "https://api.kilo.ai/api/gateway/v1";

/// Default model for cloud-dispatch tasks.
pub const DEFAULT_MODEL: &str = "minimax/minimax-m3";

/// Gateway configuration.
#[derive(Debug, Clone)]
pub struct KiloGatewayConfig {
    /// Chat completions endpoint (includes `/chat/completions` suffix path base).
    pub gateway_url: String,
    /// Bearer JWT API key.
    pub api_key: String,
    /// Model identifier.
    pub model: String,
    client: Client,
}

impl KiloGatewayConfig {
    /// Load from `KILO_API_KEY` and optional overrides.
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        let api_key = std::env::var("KILO_API_KEY")
            .map_err(|e| SubstrateError::CloudDispatch(format!("KILO_API_KEY not set: {e}")))?;
        let gateway_url =
            std::env::var("KILO_GATEWAY_URL").unwrap_or_else(|_| DEFAULT_GATEWAY_URL.to_string());
        let model = std::env::var("KILO_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        Ok(Self {
            gateway_url,
            api_key,
            model,
            client: Client::new(),
        })
    }

    /// Call the gateway with a system+user prompt and return assistant text.
    pub async fn complete(&self, system: &str, user: &str) -> Result<String> {
        let url = format!(
            "{}/chat/completions",
            self.gateway_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "max_tokens": 8192
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("kilo gateway request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SubstrateError::CloudDispatch(format!(
                "kilo gateway failed ({status}): {text}"
            )));
        }

        let parsed: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("kilo gateway parse: {e}")))?;

        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| SubstrateError::CloudDispatch("kilo gateway empty response".into()))
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}
