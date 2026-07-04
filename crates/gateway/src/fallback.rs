//! Provider fallback chain: try a primary provider, then iterate fallbacks on
//! retriable errors (5xx upstream status or connection failure).

use crate::config::ProviderConfig;

/// Describes a primary provider and an ordered list of fallback providers to
/// try when the primary returns a retriable error.
///
/// This maps directly to the `substrate.toml` provider block:
/// ```toml
/// [[providers]]
/// name = "openai"
/// url = "https://api.openai.com"
/// fallbacks = ["anthropic", "local"]
/// ```
#[derive(Debug, Clone)]
pub struct FallbackChain {
    /// Name of the primary provider to attempt first.
    pub primary: String,
    /// Ordered list of fallback provider names to try when `primary` fails.
    pub fallbacks: Vec<String>,
}

impl FallbackChain {
    /// Build a chain from a [`ProviderConfig`] (primary = config.name,
    /// fallbacks = config.fallbacks).
    pub fn from_provider_config(config: &ProviderConfig) -> Self {
        Self {
            primary: config.name.clone(),
            fallbacks: config.fallbacks.clone(),
        }
    }
}

/// Returns `true` for error strings that represent retriable upstream failures:
/// - 5xx HTTP status returned by the upstream (`"returned 5xx"` in the message)
/// - Connection / transport failures (`"request to … failed"`)
fn is_retriable(err: &str) -> bool {
    // "upstream provider X returned 500: ..."
    // "upstream provider X returned 503: ..."
    let has_5xx = err.contains("returned 5");
    // "upstream request to X failed: ..."  (connection/timeout error)
    // "upstream streaming request to X failed: ..."
    let has_conn_fail = err.contains("request to") && err.contains("failed");
    has_5xx || has_conn_fail
}

/// Try the primary provider in `chain`, then each fallback in order, using
/// `dispatch` to perform the actual request for a given [`ProviderConfig`].
///
/// - On primary success: returns `Ok` immediately (no fallbacks attempted).
/// - On retriable error (5xx / connection failure): logs at INFO level and
///   tries the next provider.
/// - On non-retriable error from the primary: returns the error immediately
///   without consulting fallbacks.
/// - If every provider in the chain fails with a retriable error: returns the
///   last error.
///
/// # Errors
/// Returns `Err(String)` when all providers are exhausted or a non-retriable
/// error is encountered.
pub async fn try_with_fallback<F, Fut, T>(
    chain: &FallbackChain,
    providers: &[ProviderConfig],
    dispatch: F,
) -> Result<T, String>
where
    F: Fn(&ProviderConfig) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let all_names: Vec<&str> = std::iter::once(chain.primary.as_str())
        .chain(chain.fallbacks.iter().map(String::as_str))
        .collect();

    let mut last_err = String::from("fallback chain is empty");

    for provider_name in &all_names {
        let Some(provider) = providers.iter().find(|p| p.name == *provider_name) else {
            eprintln!(
                "[gateway] INFO: fallback provider '{provider_name}' not found in registry, skipping"
            );
            continue;
        };

        match dispatch(provider).await {
            Ok(result) => {
                if *provider_name != chain.primary {
                    eprintln!(
                        "[gateway] INFO: fallback to provider '{}' succeeded",
                        provider_name
                    );
                }
                return Ok(result);
            }
            Err(e) if is_retriable(&e) => {
                eprintln!(
                    "[gateway] INFO: provider '{}' returned retriable error, trying next fallback: {}",
                    provider_name, e
                );
                last_err = e;
            }
            Err(e) => {
                // Non-retriable — surface immediately.
                return Err(e);
            }
        }
    }

    Err(last_err)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use std::sync::{Arc, Mutex};

    fn make_provider(name: &str) -> ProviderConfig {
        ProviderConfig::new(name, format!("https://{name}.example.com/v1"), "DUMMY_KEY")
    }

    fn make_chain(primary: &str, fallbacks: &[&str]) -> FallbackChain {
        FallbackChain {
            primary: primary.to_string(),
            fallbacks: fallbacks.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Helper: builds a dispatch closure that returns pre-configured outcomes.
    /// `outcomes` is a list of `(provider_name, Result<&str, &str>)`.
    fn make_dispatch(
        outcomes: Vec<(&'static str, Result<&'static str, &'static str>)>,
    ) -> (
        impl Fn(&ProviderConfig) -> std::future::Ready<Result<String, String>> + Clone,
        Arc<Mutex<Vec<String>>>,
    ) {
        let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = Arc::clone(&calls);
        let outcomes = Arc::new(outcomes);
        let dispatch = move |p: &ProviderConfig| {
            let name = p.name.clone();
            calls_clone.lock().unwrap().push(name.clone());
            let result = outcomes
                .iter()
                .find(|(n, _)| *n == name.as_str())
                .map(|(_, r)| r.clone())
                .unwrap_or(Err("provider not in outcomes"));
            std::future::ready(result.map(|s| s.to_string()).map_err(|e| e.to_string()))
        };
        (dispatch, calls)
    }

    // 1. Primary succeeds — no fallback attempted.
    #[tokio::test]
    async fn primary_success_no_fallback_attempted() {
        let providers = vec![make_provider("openai"), make_provider("anthropic")];
        let chain = make_chain("openai", &["anthropic"]);
        let (dispatch, calls) = make_dispatch(vec![("openai", Ok("ok"))]);

        let result = try_with_fallback(&chain, &providers, dispatch).await;
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(*calls.lock().unwrap(), vec!["openai"]);
    }

    // 2. Primary fails with 5xx → first fallback succeeds.
    #[tokio::test]
    async fn primary_5xx_fallback_first_succeeds() {
        let providers = vec![make_provider("openai"), make_provider("anthropic")];
        let chain = make_chain("openai", &["anthropic"]);
        let (dispatch, calls) = make_dispatch(vec![
            (
                "openai",
                Err("upstream provider openai returned 500: internal"),
            ),
            ("anthropic", Ok("fallback-ok")),
        ]);

        let result = try_with_fallback(&chain, &providers, dispatch).await;
        assert_eq!(result.unwrap(), "fallback-ok");
        assert_eq!(*calls.lock().unwrap(), vec!["openai", "anthropic"]);
    }

    // 3. All providers fail → last error is returned.
    #[tokio::test]
    async fn all_fail_returns_last_error() {
        let providers = vec![
            make_provider("openai"),
            make_provider("anthropic"),
            make_provider("local"),
        ];
        let chain = make_chain("openai", &["anthropic", "local"]);
        let (dispatch, calls) = make_dispatch(vec![
            ("openai", Err("upstream provider openai returned 503: down")),
            (
                "anthropic",
                Err("upstream provider anthropic returned 502: bad"),
            ),
            ("local", Err("upstream provider local returned 500: error")),
        ]);

        let result = try_with_fallback(&chain, &providers, dispatch).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("local"),
            "last error should be from local: {err}"
        );
        assert_eq!(*calls.lock().unwrap(), vec!["openai", "anthropic", "local"]);
    }

    // 4. Fallback chain with 3 providers — second fallback needed.
    #[tokio::test]
    async fn three_provider_chain_second_fallback_wins() {
        let providers = vec![make_provider("a"), make_provider("b"), make_provider("c")];
        let chain = make_chain("a", &["b", "c"]);
        let (dispatch, calls) = make_dispatch(vec![
            ("a", Err("upstream provider a returned 500: err")),
            ("b", Err("upstream request to b failed: timeout")),
            ("c", Ok("c-wins")),
        ]);

        let result = try_with_fallback(&chain, &providers, dispatch).await;
        assert_eq!(result.unwrap(), "c-wins");
        assert_eq!(*calls.lock().unwrap(), vec!["a", "b", "c"]);
    }

    // 5. Empty fallback list — only primary is tried; its error is returned.
    #[tokio::test]
    async fn empty_fallback_list_only_primary_tried() {
        let providers = vec![make_provider("openai")];
        let chain = make_chain("openai", &[]);
        let (dispatch, calls) = make_dispatch(vec![(
            "openai",
            Err("upstream provider openai returned 500: bad"),
        )]);

        let result = try_with_fallback(&chain, &providers, dispatch).await;
        assert!(result.is_err());
        assert_eq!(*calls.lock().unwrap(), vec!["openai"]);
    }

    // 6. Non-retriable error (4xx) — does not consult fallbacks.
    #[tokio::test]
    async fn non_retriable_error_skips_fallback() {
        let providers = vec![make_provider("openai"), make_provider("anthropic")];
        let chain = make_chain("openai", &["anthropic"]);
        let (dispatch, calls) = make_dispatch(vec![
            (
                "openai",
                Err("upstream provider openai returned 401: unauthorized"),
            ),
            ("anthropic", Ok("should-not-reach")),
        ]);

        let result = try_with_fallback(&chain, &providers, dispatch).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("401"), "must surface 401: {err}");
        // anthropic must NOT have been called
        assert_eq!(*calls.lock().unwrap(), vec!["openai"]);
    }

    // 7. is_retriable covers connection failures.
    #[test]
    fn is_retriable_connection_error() {
        assert!(is_retriable(
            "upstream request to deepseek failed: connection refused"
        ));
        assert!(is_retriable(
            "upstream streaming request to openai failed: timeout"
        ));
        assert!(is_retriable(
            "upstream provider foo returned 503: Service Unavailable"
        ));
        assert!(!is_retriable(
            "upstream provider foo returned 401: Unauthorized"
        ));
        assert!(!is_retriable("API key not available for provider foo"));
    }

    // 8. FallbackChain::from_provider_config round-trips correctly.
    #[test]
    fn fallback_chain_from_provider_config() {
        let config = make_provider("openai").with_fallbacks(["anthropic", "local"]);
        let chain = FallbackChain::from_provider_config(&config);
        assert_eq!(chain.primary, "openai");
        assert_eq!(chain.fallbacks, vec!["anthropic", "local"]);
    }
}
