//! Pure parsing helpers (no IO) so they can be unit-tested directly.

use std::sync::OnceLock;

use regex::Regex;
use substrate_core::domain::{ConversationDump, Part, StructuredResult, TaskState};
use substrate_core::error::{Result, SubstrateError};
use uuid::Uuid;

fn conv_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)conversation[ _-]?id[:=]?\s*([0-9a-f-]{8,})").unwrap()
    })
}

fn uuid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
    })
}

fn pr_url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"https://github\.com/[^ )"']+/pull/\d+"#).unwrap()
    })
}

/// Extract a conversation id from engine stdout.
///
/// Prefers an explicit `conversation-id: ...` token; falls back to the first
/// bare UUID anywhere in the text.
pub fn extract_conversation_id(stdout: &str) -> Option<String> {
    if let Some(c) = conv_id_re().captures(stdout) {
        return Some(c[1].to_string());
    }
    uuid_re().find(stdout).map(|m| m.as_str().to_string())
}

/// Find every GitHub pull-request URL in `text`, de-duplicated, in order.
pub fn extract_pr_urls(text: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for m in pr_url_re().find_iter(text) {
        let s = m.as_str().to_string();
        if !seen.contains(&s) {
            seen.push(s);
        }
    }
    seen
}

/// Shape of a `forge conversation dump` JSON payload (tolerant subset).
#[derive(Debug, serde::Deserialize)]
struct ForgeDump {
    #[serde(default)]
    messages: Vec<ForgeMessage>,
    /// Optional explicit exit code the engine reported.
    #[serde(default)]
    exit_code: Option<i32>,
}

#[derive(Debug, serde::Deserialize)]
struct ForgeMessage {
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: String,
}

/// Normalize a raw forge dump into a [`StructuredResult`].
///
/// - `text`   = the last assistant message content.
/// - `pr_urls`= every GitHub PR URL across all message content.
/// - `status` = `Completed` if a `DONE:` marker or a PR URL is present, or if
///   `exit_code == 0`; otherwise `Failed`.
pub fn parse_dump(dump: &ConversationDump) -> Result<StructuredResult> {
    let parsed: ForgeDump = serde_json::from_str(&dump.raw)
        .map_err(|e| SubstrateError::Engine(format!("forge dump parse: {e}")))?;

    let all_text: String = parsed
        .messages
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let text = parsed
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let pr_urls = extract_pr_urls(&all_text);

    let has_done = all_text.contains("DONE:");
    let status = if has_done || !pr_urls.is_empty() || parsed.exit_code == Some(0) {
        TaskState::Completed
    } else {
        TaskState::Failed
    };

    let artifacts: Vec<Part> = pr_urls
        .iter()
        .map(|u| Part::Artifact {
            name: "pull_request".to_string(),
            uri: u.clone(),
        })
        .collect();

    Ok(StructuredResult {
        text,
        artifacts,
        pr_urls,
        status,
    })
}

/// Generate a fallback conversation id when none can be parsed.
pub fn fallback_conversation_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_conversation_id() {
        let s = "starting...\nconversation-id: 11111111-1111-1111-1111-111111111111\nok";
        assert_eq!(
            extract_conversation_id(s).as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
    }

    #[test]
    fn parses_loose_conversation_id_variants() {
        // Underscore separator + '=' immediately after id (spec regex shape).
        assert_eq!(
            extract_conversation_id("Conversation_ID=abcdef12").as_deref(),
            Some("abcdef12")
        );
        // Space separator, no delimiter, whitespace before the id.
        assert_eq!(
            extract_conversation_id("CONVERSATION ID deadbeefcafe").as_deref(),
            Some("deadbeefcafe")
        );
        // Hyphen separator with colon delimiter.
        assert_eq!(
            extract_conversation_id("conversation-id: 0a1b2c3d").as_deref(),
            Some("0a1b2c3d")
        );
    }

    #[test]
    fn falls_back_to_bare_uuid_line() {
        let s = "no labelled id here\n22222222-3333-4444-5555-666666666666\n";
        assert_eq!(
            extract_conversation_id(s).as_deref(),
            Some("22222222-3333-4444-5555-666666666666")
        );
    }

    #[test]
    fn returns_none_when_absent() {
        assert!(extract_conversation_id("nothing useful").is_none());
    }

    #[test]
    fn extracts_and_dedups_pr_urls() {
        let t = "see https://github.com/a/b/pull/12 and https://github.com/a/b/pull/12 also https://github.com/x/y/pull/7";
        assert_eq!(
            extract_pr_urls(t),
            vec![
                "https://github.com/a/b/pull/12".to_string(),
                "https://github.com/x/y/pull/7".to_string()
            ]
        );
    }
}
