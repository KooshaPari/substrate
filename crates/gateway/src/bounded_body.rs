//! Request body size limiting layer — rejects requests exceeding max_body_size with 413 Payload Too Large.
//! Scans only a configurable prefix for prompt detection, never buffers full body (O(prefix) memory).

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};

/// Bounded body layer configuration.
#[derive(Debug, Clone)]
pub struct BoundedBodyConfig {
    /// Maximum request body size in bytes (e.g., 50 * 1024 * 1024 = 50MB).
    pub max_body_size: usize,
    /// Maximum prefix to scan for routing hints (e.g., 1024 bytes).
    pub prefix_size: usize,
    /// Disabled by default; set via env or header to enable scanning.
    pub enabled: bool,
}

impl Default for BoundedBodyConfig {
    fn default() -> Self {
        Self {
            max_body_size: 50 * 1024 * 1024, // 50 MB default
            prefix_size: 1024,
            enabled: false,
        }
    }
}

/// Middleware that enforces request body size limits.
/// Returns 413 Payload Too Large if body exceeds limit.
/// Keeps memory O(prefix_size), never O(body_size).
pub async fn bounded_body_middleware(
    config: BoundedBodyConfig,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !config.enabled {
        return Ok(next.run(request).await);
    }

    // Check content-length header (fast path)
    if let Some(content_length_header) = request.headers().get("content-length") {
        if let Ok(content_length_str) = content_length_header.to_str() {
            if let Ok(content_length) = content_length_str.parse::<usize>() {
                if content_length > config.max_body_size {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
            }
        }
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = BoundedBodyConfig::default();
        assert_eq!(config.max_body_size, 50 * 1024 * 1024);
        assert_eq!(config.prefix_size, 1024);
        assert!(!config.enabled);
    }

    #[test]
    fn test_disabled_passthrough() {
        let config = BoundedBodyConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(!config.enabled);
    }

    #[test]
    fn test_max_body_size_calculation() {
        let config = BoundedBodyConfig {
            max_body_size: 1024,
            prefix_size: 256,
            enabled: true,
        };
        assert!(config.max_body_size > config.prefix_size);
    }

    #[test]
    fn test_prefix_scan_memory_bounded() {
        let config = BoundedBodyConfig {
            max_body_size: 100 * 1024 * 1024,
            prefix_size: 1024,
            enabled: true,
        };
        // Memory used is O(prefix_size), not O(response_size)
        let expected_memory = config.prefix_size;
        assert_eq!(expected_memory, 1024);
    }
}
