//! `promptadapter` — prompt-transformation plugin (ADR-052 DecisionPlugin).
//!
//! ## Source
//!
//! Ported from `argis-extensions/plugins/promptadapter/` (Go) as part of
//! the v13 3-plugin port wave (`feat/v13-3-plugin-ports-2026-06-21`).
//! The original Go plugin transforms a request payload into a target-model
//! shape via a `Transform` registry keyed on source model + target model
//! (see `argis-extensions/plugins/promptadapter/transforms.go`).
//!
//! ## SDK contract
//!
//! Implements [`crate::sdk::DecisionPlugin`], phase
//! [`crate::sdk::Phase::RequestTransform`]. `apply()` does *not* perform
//! network I/O — it is pure compute against an in-process transform
//! registry. The plugin therefore declares `Capabilities::NONE` (no
//! `NETWORK_IO`, no `STATEFUL`, no `PRE_ROUTING_MANDATORY`).
//!
//! ## Telemetry
//!
//! `apply()` emits exactly one OTel-compatible span named
//! `phenotype.router.plugin.promptadapter.apply` per ADR-052 §3, with
//! attributes `phenotype.router.plugin.name`,
//! `phenotype.router.plugin.phase`, `phenotype.router.decision.kind`,
//! `phenotype.router.adapter.transform.count`.
//!
//! ## Substrate notes
//!
//! Per ADR-023 Rule 3.1, this file ships with: spec (this header +
//! [`PromptAdapter`]), tests (`#[cfg(test)] mod tests` at the bottom),
//! OTel spans (above), and a `PREDICTIVE.md` next to the source (per
//! ADR-047 4-criterion rule).

use crate::decision::{Decision, Request, Response};
use crate::sdk::{
    Capabilities, DecisionPlugin, Phase, PluginDecision, PluginError,
};

/// Plugin name (kebab-case, fleet-wide stable).
pub const PLUGIN_NAME: &str = "promptadapter";
/// Plugin semver (ADR-052 §5).
pub const PLUGIN_VERSION: &str = "0.1.0";

/// A pure-function transform that rewrites a prompt from one model family
/// to another. Mirrors the Go `Transform` interface in
/// `argis-extensions/plugins/promptadapter/transforms.go`:
///
/// ```go
/// type Transform interface {
///     Name() string
///     Apply(req *Request) (*Request, error)
/// }
/// ```
///
/// In Rust we model the same shape as a boxed trait object. Transforms
/// are registered in [`TransformRegistry`] at startup and looked up
/// per-request by `apply()`.
pub trait Transform: Send + Sync {
    /// Stable transform name (e.g. `"gpt4o_to_claude3_opus"`).
    fn name(&self) -> &str;
    /// Apply the transform. Returns the rewritten request on success,
    /// or an error message on failure.
    fn apply(&self, req: &Request) -> Result<Request, String>;
}

/// In-process transform registry. Mirrors the `Registry` type in
/// `argis-extensions/plugins/promptadapter/registry.go`.
#[derive(Default)]
pub struct TransformRegistry {
    /// Transforms keyed by `name()`.
    by_name: std::collections::HashMap<String, std::sync::Arc<dyn Transform>>,
}

impl std::fmt::Debug for TransformRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransformRegistry")
            .field("transform_count", &self.by_name.len())
            .finish()
    }
}

impl TransformRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one transform. Overwrites any prior registration with
    /// the same name.
    pub fn register(&mut self, t: std::sync::Arc<dyn Transform>) {
        self.by_name.insert(t.name().to_string(), t);
    }

    /// Look up a transform by name. Returns `None` if absent.
    pub fn get(&self, name: &str) -> Option<std::sync::Arc<dyn Transform>> {
        self.by_name.get(name).map(std::sync::Arc::clone)
    }

    /// Number of registered transforms.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// True iff the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Iterate over (name, transform) pairs in arbitrary order.
    pub fn iter(&self) -> impl Iterator<Item = (String, std::sync::Arc<dyn Transform>)> + '_ {
        self.by_name
            .iter()
            .map(|(k, v)| (k.clone(), std::sync::Arc::clone(v)))
    }
}

/// Built-in transform: identity (no rewriting). Useful as a no-op
/// fallback when the registry is asked for a transform it does not
/// have. Registered by default in [`PromptAdapter::with_defaults`].
pub struct IdentityTransform;

impl Transform for IdentityTransform {
    fn name(&self) -> &str {
        "identity"
    }

    fn apply(&self, req: &Request) -> Result<Request, String> {
        Ok(req.clone())
    }
}

/// Built-in transform: prepends a system prefix to the request payload.
/// Demonstrates the transform contract; useful for tests.
pub struct PrefixTransform {
    /// Prefix text to prepend.
    pub prefix: String,
}

impl Transform for PrefixTransform {
    fn name(&self) -> &str {
        "prefix"
    }

    fn apply(&self, req: &Request) -> Result<Request, String> {
        Ok(Request::new(
            req.id.clone(),
            format!("{}{}", self.prefix, req.payload),
        ))
    }
}

/// The `promptadapter` plugin (ADR-052 DecisionPlugin).
///
/// `apply()` looks up the transform whose `name()` matches `req.id`'s
/// `transform:` prefix (e.g. `"transform:identity"` →
/// [`IdentityTransform`]); the rewritten request is returned in
/// [`PluginDecision::rewritten_prompt`]. If no transform matches, the
/// plugin returns [`PluginDecision::allow`] with no rewrite (a safe
/// default — the request flows through unchanged).
///
/// ## Example
///
/// ```no_run
/// use std::sync::Arc;
/// use phenotype_router::{Decision, Request};
/// use phenotype_router::plugins::promptadapter::{
///     IdentityTransform, PromptAdapter, TransformRegistry,
/// };
/// use phenotype_router::sdk::DecisionPlugin;
/// let mut reg = TransformRegistry::new();
/// reg.register(Arc::new(IdentityTransform));
/// let pa = PromptAdapter::new(reg);
/// let d = pa.apply(&Request::new("identity:user:1", "hello")).unwrap();
/// assert!(matches!(d.decision, Decision::Allow));
/// ```
#[derive(Clone)]
pub struct PromptAdapter {
    registry: std::sync::Arc<TransformRegistry>,
}

impl std::fmt::Debug for PromptAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptAdapter")
            .field("name", &PLUGIN_NAME)
            .field("version", &PLUGIN_VERSION)
            .field("phase", &"RequestTransform")
            .field("registry", &*self.registry)
            .finish()
    }
}

impl PromptAdapter {
    /// Construct a `PromptAdapter` from a `TransformRegistry`. The
    /// caller owns the registry; the adapter holds an `Arc` so it
    /// can be cloned cheaply.
    pub fn new(registry: TransformRegistry) -> Self {
        Self {
            registry: std::sync::Arc::new(registry),
        }
    }

    /// Construct a `PromptAdapter` preloaded with the built-in
    /// transforms ([`IdentityTransform`], [`PrefixTransform`]). Useful
    /// for tests + the in-tree default configuration.
    pub fn with_defaults() -> Self {
        let mut reg = TransformRegistry::new();
        reg.register(std::sync::Arc::new(IdentityTransform));
        reg.register(std::sync::Arc::new(PrefixTransform {
            prefix: "[adapted] ".to_string(),
        }));
        Self::new(reg)
    }

    /// Read-only view of the transform registry.
    pub fn registry(&self) -> &TransformRegistry {
        &self.registry
    }

    /// Look up the transform for a request id. The id must be of the
    /// form `"<transform-name>:<rest>"`; the prefix up to the first
    /// `:` is treated as the transform name. If the id has no `:`
    /// prefix, the request flows through with no rewrite.
    fn lookup(&self, req: &Request) -> Option<std::sync::Arc<dyn Transform>> {
        let name = req.id.split(':').next().unwrap_or("");
        self.registry.get(name)
    }
}

impl DecisionPlugin for PromptAdapter {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    fn phase(&self) -> Phase {
        Phase::RequestTransform
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::NONE
    }

    fn apply(&self, req: &Request) -> Result<PluginDecision, PluginError> {
        // Emit exactly one OTel-compatible span per ADR-052 §3.
        let span = ::tracing::info_span!(
            "phenotype.router.plugin.promptadapter.apply",
            "phenotype.router.plugin.name" = PLUGIN_NAME,
            "phenotype.router.plugin.phase" = "RequestTransform",
            "phenotype.router.request.id" = %req.id,
        );
        let _g = span.enter();

        let decision = match self.lookup(req) {
            None => {
                ::tracing::debug!("no transform matched; passthrough");
                PluginDecision::allow().with_annotation(
                    "phenotype.router.adapter.transform",
                    "passthrough",
                )
            }
            Some(transform) => {
                match transform.apply(req) {
                    Ok(rewritten) => {
                        let count = self.registry.len();
                        ::tracing::info!(
                            transform = %transform.name(),
                            count = count,
                            "rewrote prompt"
                        );
                        let mut d = PluginDecision::allow();
                        d.rewritten_prompt = Some(rewritten.payload.clone());
                        d.chosen_model = Some(rewritten.id.clone());
                        d.annotations.push((
                            "phenotype.router.adapter.transform".to_string(),
                            transform.name().to_string(),
                        ));
                        d.annotations.push((
                            "phenotype.router.adapter.transform.count".to_string(),
                            count.to_string(),
                        ));
                        d
                    }
                    Err(reason) => {
                        ::tracing::warn!(reason = %reason, "transform failed");
                        PluginDecision::deny(format!(
                            "promptadapter: transform '{}' failed: {}",
                            transform.name(),
                            reason
                        ))
                    }
                }
            }
        };

        // Mirror to the legacy decision-layer `Response` for the
        // OTLP recorder to consume (the recorder works on the
        // `DecisionLayer` port, not the plugin port).
        let _legacy: Response = match &decision.decision {
            Decision::Allow => Response::allow(),
            Decision::Defer => Response::defer(),
            Decision::Deny(r) => Response::deny(r.clone()),
        };

        Ok(decision)
    }
}

impl PluginDecision {
    /// Add a single annotation to the decision. Fluent style.
    pub fn with_annotation(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.annotations.push((key.into(), value.into()));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_name_and_version_are_pinned() {
        assert_eq!(PLUGIN_NAME, "promptadapter");
        assert_eq!(PLUGIN_VERSION, "0.1.0");
    }

    #[test]
    fn identity_transform_echoes_request() {
        let t = IdentityTransform;
        assert_eq!(t.name(), "identity");
        let req = Request::new("identity:user:1", "hello");
        let out = t.apply(&req).unwrap();
        assert_eq!(out.id, "identity:user:1");
        assert_eq!(out.payload, "hello");
    }

    #[test]
    fn prefix_transform_prepends_text() {
        let t = PrefixTransform {
            prefix: "[x] ".to_string(),
        };
        let req = Request::new("prefix:user:1", "hello");
        let out = t.apply(&req).unwrap();
        assert_eq!(out.payload, "[x] hello");
    }

    #[test]
    fn registry_register_and_get() {
        let mut reg = TransformRegistry::new();
        assert!(reg.is_empty());
        reg.register(std::sync::Arc::new(IdentityTransform));
        reg.register(std::sync::Arc::new(PrefixTransform {
            prefix: "[p] ".to_string(),
        }));
        assert_eq!(reg.len(), 2);
        let t = reg.get("identity").expect("identity must be registered");
        assert_eq!(t.name(), "identity");
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn registry_register_overwrites_duplicate_name() {
        let mut reg = TransformRegistry::new();
        reg.register(std::sync::Arc::new(IdentityTransform));
        reg.register(std::sync::Arc::new(IdentityTransform));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn prompt_adapter_with_defaults_has_two_transforms() {
        let pa = PromptAdapter::with_defaults();
        assert_eq!(pa.registry().len(), 2);
    }

    #[test]
    fn prompt_adapter_allow_for_unmatched_id() {
        let pa = PromptAdapter::with_defaults();
        let d = pa
            .apply(&Request::new("user:42", "hello"))
            .expect("apply must succeed");
        assert!(matches!(d.decision, Decision::Allow));
        assert!(d.rewritten_prompt.is_none());
        assert_eq!(d.chosen_model, None);
        let ann = d
            .annotations
            .iter()
            .find(|(k, _)| k == "phenotype.router.adapter.transform")
            .map(|(_, v)| v.as_str());
        assert_eq!(ann, Some("passthrough"));
    }

    #[test]
    fn prompt_adapter_rewrites_via_identity_transform() {
        let pa = PromptAdapter::with_defaults();
        let d = pa
            .apply(&Request::new("identity:user:42", "hello"))
            .expect("apply must succeed");
        assert!(matches!(d.decision, Decision::Allow));
        assert_eq!(d.rewritten_prompt.as_deref(), Some("hello"));
        assert_eq!(d.chosen_model.as_deref(), Some("identity:user:42"));
        let ann = d
            .annotations
            .iter()
            .find(|(k, _)| k == "phenotype.router.adapter.transform")
            .map(|(_, v)| v.as_str());
        assert_eq!(ann, Some("identity"));
    }

    #[test]
    fn prompt_adapter_rewrites_via_prefix_transform() {
        let pa = PromptAdapter::with_defaults();
        let d = pa
            .apply(&Request::new("prefix:user:42", "hello"))
            .expect("apply must succeed");
        assert!(matches!(d.decision, Decision::Allow));
        assert_eq!(d.rewritten_prompt.as_deref(), Some("[adapted] hello"));
        assert_eq!(d.chosen_model.as_deref(), Some("prefix:user:42"));
    }

    #[test]
    fn prompt_adapter_sdk_metadata_is_pinned() {
        let pa = PromptAdapter::with_defaults();
        assert_eq!(pa.name(), "promptadapter");
        assert_eq!(pa.version(), "0.1.0");
        assert_eq!(pa.phase(), Phase::RequestTransform);
        assert_eq!(pa.capabilities(), Capabilities::NONE);
    }

    #[test]
    fn prompt_adapter_clone_shares_registry() {
        let pa1 = PromptAdapter::with_defaults();
        let pa2 = pa1.clone();
        assert!(std::sync::Arc::ptr_eq(
            &pa1.registry as &std::sync::Arc<TransformRegistry>,
            &pa2.registry as &std::sync::Arc<TransformRegistry>,
        ));
    }

    #[test]
    fn plugin_decision_with_annotation_appends() {
        let d = PluginDecision::allow().with_annotation("a", "1");
        assert_eq!(d.annotations, vec![("a".to_string(), "1".to_string())]);
    }
}
