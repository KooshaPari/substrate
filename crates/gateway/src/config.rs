//! HTTP server configuration from environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Authentication scheme for an upstream provider.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AuthScheme {
    /// Standard OpenAI-style `Authorization: Bearer <key>`.
    #[default]
    Bearer,
}

/// Configuration for a single upstream provider (OpenAI-compatible passthrough).
///
/// API keys are **never** stored here directly — only the environment variable
/// name is stored. Keys are resolved at request time via [`std::env::var`].
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Short identifier, e.g. `"deepseek"` or `"kilocode"`.
    pub name: String,
    /// Base URL of the OpenAI-compatible endpoint (no trailing slash).
    pub base_url: String,
    /// Name of the environment variable that holds the API key.
    pub api_key_env: String,
    /// Authentication scheme to use when calling the upstream.
    pub auth_scheme: AuthScheme,
    /// Default model to use when the request does not specify one.
    pub default_model: Option<String>,
}

impl ProviderConfig {
    /// Build a provider config.  `api_key_env` is the env-var **name**, not the key itself.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key_env: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            api_key_env: api_key_env.into(),
            auth_scheme: AuthScheme::Bearer,
            default_model: None,
        }
    }

    /// Override the auth scheme.
    pub fn with_auth_scheme(mut self, scheme: AuthScheme) -> Self {
        self.auth_scheme = scheme;
        self
    }

    /// Set a default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }

    /// Resolve the API key from the environment at runtime.
    /// Returns `None` (and logs a warning) if the variable is not set.
    pub fn resolve_api_key(&self) -> Option<String> {
        match std::env::var(&self.api_key_env) {
            Ok(k) if !k.trim().is_empty() => Some(k),
            Ok(_) => {
                eprintln!(
                    "[gateway] WARNING: env var {} is set but empty; skipping provider {}",
                    self.api_key_env, self.name
                );
                None
            }
            Err(_) => {
                eprintln!(
                    "[gateway] WARNING: env var {} not set; provider {} will be unavailable",
                    self.api_key_env, self.name
                );
                None
            }
        }
    }
}

/// Returns the built-in provider configs.
///
/// Keys are **never** hardcoded here — only env-var names are listed.
pub fn builtin_providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig::new(
            "opencode-go",
            "https://opencode.ai/zen/go/v1",
            "OPENCODE_API_KEY",
        )
        .with_default_model("deepseek-v4-flash"),
        ProviderConfig::new("deepseek", "https://api.deepseek.com/v1", "DEEPSEEK_API_KEY"),
        ProviderConfig::new(
            "kilocode",
            "https://api.kilo.ai/api/gateway/v1",
            "KILOCODE_API_KEY",
        ),
    ]
}

/// Route a model string to a provider using prefix matching.
///
/// If the model contains a `/`, the part before the `/` is matched against
/// registered provider names.  Returns `(provider, stripped_model)` on a hit,
/// or `None` if no prefix matches (caller falls through to OmniRoute).
pub fn resolve_provider<'a>(
    providers: &'a [ProviderConfig],
    model: &str,
) -> Option<(&'a ProviderConfig, String)> {
    if let Some(slash) = model.find('/') {
        let prefix = &model[..slash];
        let stripped = model[slash + 1..].to_string();
        if let Some(p) = providers.iter().find(|p| p.name == prefix) {
            return Some((p, stripped));
        }
    }
    None
}

/// Runtime configuration for the substrate gateway.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Socket address to bind (e.g. `127.0.0.1:20128`).
    pub bind: SocketAddr,
    /// Root directory for `.substrate` state (sqlite stores).
    pub state_dir: PathBuf,
    /// Optional bearer token; when set, protected routes require auth.
    pub auth_token: Option<String>,
    /// Upstream provider configurations (keys read from env at runtime).
    pub providers: Vec<ProviderConfig>,
}

impl GatewayConfig {
    /// Load configuration from the process environment.
    ///
    /// | Variable | Default |
    /// |----------|---------|
    /// | `SUBSTRATE_GATEWAY_BIND` | `127.0.0.1:20128` |
    /// | `SUBSTRATE_STATE_DIR` | `./.substrate` |
    /// | `SUBSTRATE_GATEWAY_AUTH_TOKEN` | unset (no auth) |
    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        let bind = std::env::var("SUBSTRATE_GATEWAY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:20128".into())
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid SUBSTRATE_GATEWAY_BIND: {e}"))?;
        let state_dir = std::env::var("SUBSTRATE_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".substrate"));
        let auth_token = std::env::var("SUBSTRATE_GATEWAY_AUTH_TOKEN").ok();
        let providers = builtin_providers();
        Ok(GatewayConfig {
            bind,
            state_dir,
            auth_token,
            providers,
        })
    }
}
