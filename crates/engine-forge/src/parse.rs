//! Pure parsing helpers (no IO) so they can be unit-tested directly.

use std::sync::OnceLock;

use regex::Regex;
use substrate_core::domain::{ConversationDump, Part, StructuredResult, TaskState};
use substrate_core::error::{Result, SubstrateError};
use uuid::Uuid;

fn conv_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)conversation[ _-]?id[:=]?\s*([0-9a-f-]{8,})").unwrap())
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
    RE.get_or_init(|| Regex::new(r#"https://github\.com/[^ )"']+/pull/\d+"#).unwrap())
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
/// Status priority (highest first):
/// 1. `exit_code == 0` AND assistant text contains `"max steps"` -> [`TaskState::Failed`]
///    (forge's "max steps" signal is a soft failure, not a clean done).
/// 2. Non-zero `exit_code` (without max-steps) -> [`TaskState::Failed`].
/// 3. `DONE:` marker or at least one PR URL -> [`TaskState::Completed`].
/// 4. Otherwise -> [`TaskState::Failed`].
///
/// `text` is the last assistant message content; `pr_urls` is the
/// de-duplicated, ordered list of GitHub PR URLs across all messages.
pub fn parse_dump(dump: &ConversationDump) -> Result<StructuredResult> {
    let parsed: ForgeDump = serde_json::from_str(&dump.raw).map_err(|e| {
        SubstrateError::Engine(format!(
            "forge dump parse failed (JSON parse error: {}); raw output: {}",
            e,
            &dump.raw[..std::cmp::min(500, dump.raw.len())]
        ))
    })?;

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

    let maxed = all_text.to_lowercase().contains("max steps");
    let nonzero_exit = parsed.exit_code.is_some_and(|c| c != 0);

    let status = if maxed {
        // The "max steps" signal in forge is treated as a soft failure: the
        // task is exhausted, not cleanly done. The dump is still useful.
        TaskState::Failed
    } else if nonzero_exit {
        TaskState::Failed
    } else if all_text.contains("DONE:") || !pr_urls.is_empty() {
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

/// The authoritative fallback for conversation-id capture.
///
/// When the regex/UUID parse of the child's stdout fails, we snapshot
/// `forge conversation list` before spawning and again after, then return
/// the first id that is new in the after-snapshot. This handles cases
/// where forge's stdout is fully captured by a TUI / piped elsewhere and
/// the labelled token is swallowed.
///
/// `before` and `after` are the two list snapshots; each is any iterable
/// of conversation id strings (we don't constrain the exact wire format).
/// Returns `Some(id)` when exactly one new id is present, `None` when
/// the diff is empty or ambiguous (multiple new ids — we refuse to guess).
pub fn find_new_conversation_id<'a, I, J>(before: I, after: J) -> Option<String>
where
    I: IntoIterator<Item = &'a str>,
    J: IntoIterator<Item = &'a str>,
{
    let before: std::collections::HashSet<&str> = before.into_iter().collect();
    let mut new_ids: Vec<String> = after
        .into_iter()
        .filter(|id| !before.contains(id))
        .map(|s| s.to_string())
        .collect();
    if new_ids.len() == 1 {
        Some(new_ids.remove(0))
    } else {
        // Empty diff: nothing new. Ambiguous (>=2): don't guess.
        None
    }
}

/// Parse a `forge conversation list` text snapshot into a list of ids.
///
/// Tolerant: trims whitespace, ignores blank lines and comment lines
/// (starting with `#`), and accepts either bare uuids or tokens that
/// contain a uuid (so it can handle tab-separated columns).
pub fn parse_list_snapshot(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(id) = uuid_re().find(line) {
            out.push(id.as_str().to_string());
        }
    }
    out
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

    // ---- extract_result status priority tests (Phase 1) -------------------

    #[test]
    fn max_steps_marks_failed_even_with_pr_url() {
        // Forge signals exhaustion via "max steps" in the assistant text.
        // Even when a PR URL is present, the run is considered failed
        // (the PR may have been opened by a prior step and the loop then
        // exhausted on a follow-up).
        let raw = r#"{
          "conversation_id": "33333333-3333-3333-3333-333333333333",
          "exit_code": 0,
          "messages": [
            { "role": "user", "content": "open a PR" },
            { "role": "assistant",
              "content": "Opened https://github.com/a/b/pull/9 then hit max steps on follow-up." }
          ]
        }"#;
        let dump = ConversationDump {
            conversation_id: "x".into(),
            raw: raw.into(),
        };
        let r = parse_dump(&dump).unwrap();
        assert_eq!(r.status, TaskState::Failed);
        assert_eq!(r.pr_urls, vec!["https://github.com/a/b/pull/9".to_string()]);
    }

    #[test]
    fn nonzero_exit_marks_failed() {
        let raw = r#"{
          "conversation_id": "44444444-4444-4444-4444-444444444444",
          "exit_code": 2,
          "messages": [
            { "role": "user", "content": "do thing" },
            { "role": "assistant", "content": "tried but it broke" }
          ]
        }"#;
        let dump = ConversationDump {
            conversation_id: "x".into(),
            raw: raw.into(),
        };
        let r = parse_dump(&dump).unwrap();
        assert_eq!(r.status, TaskState::Failed);
    }

    #[test]
    fn done_marker_still_marks_completed() {
        let raw = r#"{
          "conversation_id": "55555555-5555-5555-5555-555555555555",
          "exit_code": 0,
          "messages": [
            { "role": "user", "content": "x" },
            { "role": "assistant", "content": "DONE: all set" }
          ]
        }"#;
        let dump = ConversationDump {
            conversation_id: "x".into(),
            raw: raw.into(),
        };
        let r = parse_dump(&dump).unwrap();
        assert_eq!(r.status, TaskState::Completed);
    }

    // ---- conversation-id capture sample set (Phase 1) --------------------
    //
    // The spec requires ≥4 recorded stdout shapes. Each is labelled with
    // the strategy that must win.

    /// Sample 1: explicit `conversation-id: <id>` (the labelled case).
    #[test]
    fn conv_id_sample_labelled_id_wins_first_match() {
        let s = "\
[boot] forge v0.7.0
[init] workspace ready
conversation-id: aabbccdd-eeff-0011-2233-445566778899
working...";
        assert_eq!(
            extract_conversation_id(s).as_deref(),
            Some("aabbccdd-eeff-0011-2233-445566778899")
        );
    }

    /// Sample 2: a bare UUID on its own line, no label.
    #[test]
    fn conv_id_sample_bare_uuid_line_falls_back() {
        let s = "\
hello there
12345678-90ab-cdef-1234-567890abcdef
no labelled token in this stdout";
        assert_eq!(
            extract_conversation_id(s).as_deref(),
            Some("12345678-90ab-cdef-1234-567890abcdef")
        );
    }

    /// Sample 3: the id is mid-noise (surrounded by ANSI/log noise).
    #[test]
    fn conv_id_sample_id_mid_noise_still_extracted() {
        // Heavy log noise, then a labelled line; the regex must still win.
        let s = "\
\x1b[31m[error]\x1b[0m retrying
[trace] token=abc conversation_id=deadbeef-1234-5678-9abc-def012345678
[ok] heartbeat
\x1b[2;37mmore noise\x1b[0m";
        assert_eq!(
            extract_conversation_id(s).as_deref(),
            Some("deadbeef-1234-5678-9abc-def012345678")
        );
    }

    /// Sample 4: no id at all -> parser returns None, the caller falls back
    /// to a generated id (or to the `forge conversation list` diff strategy).
    #[test]
    fn conv_id_sample_no_id_returns_none() {
        let s = "\
forge v0.7.0
starting run...
no conversation identifier is printed here
done";
        assert!(extract_conversation_id(s).is_none());
    }

    /// Bonus: case-insensitive label + space/underscore/hyphen separators
    /// all keep working (this is the regex's primary path).
    #[test]
    fn conv_id_sample_label_variants() {
        assert_eq!(
            extract_conversation_id("CONVERSATION_ID: 0011223344556677").as_deref(),
            Some("0011223344556677")
        );
        assert_eq!(
            extract_conversation_id("Conversation id 9999aaaa").as_deref(),
            Some("9999aaaa")
        );
        assert_eq!(
            extract_conversation_id("conversation-id=cafe1234").as_deref(),
            Some("cafe1234")
        );
    }

    // ---- authoritative fallback: list-diff strategy ---------------------

    #[test]
    fn diff_fallback_finds_single_new_id() {
        let before = ["aaaa", "bbbb", "cccc"];
        let after = ["aaaa", "bbbb", "cccc", "dddd"];
        assert_eq!(
            find_new_conversation_id(before.iter().copied(), after.iter().copied()),
            Some("dddd".to_string())
        );
    }

    #[test]
    fn diff_fallback_returns_none_when_empty() {
        let before = ["aaaa"];
        let after = ["aaaa"];
        assert!(find_new_conversation_id(before.iter().copied(), after.iter().copied()).is_none());
    }

    #[test]
    fn diff_fallback_refuses_to_guess_when_ambiguous() {
        // Two new ids: ambiguous, the function must not silently pick one.
        let before = ["aaaa"];
        let after = ["aaaa", "bbbb", "cccc"];
        assert!(find_new_conversation_id(before.iter().copied(), after.iter().copied()).is_none());
    }

    #[test]
    fn diff_fallback_handles_empty_before() {
        let before: [&str; 0] = [];
        let after = ["first"];
        assert_eq!(
            find_new_conversation_id(before.iter().copied(), after.iter().copied()),
            Some("first".to_string())
        );
    }

    #[test]
    fn diff_fallback_works_on_realistic_list_lines() {
        // Mirror what `forge conversation list` looks like in the wild:
        // one id per line, possibly with leading whitespace / a header.
        let before = ["11111111-1111-1111-1111-111111111111"];
        let after = [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
        ];
        assert_eq!(
            find_new_conversation_id(before.iter().copied(), after.iter().copied()),
            Some("22222222-2222-2222-2222-222222222222".to_string())
        );
    }

    // ---- list-snapshot parser (forge conversation list -> Vec<String>) ---

    #[test]
    fn parses_list_snapshot_one_per_line() {
        let raw = "\
11111111-1111-1111-1111-111111111111
22222222-2222-2222-2222-222222222222
33333333-3333-3333-3333-333333333333
";
        let ids = parse_list_snapshot(raw);
        assert_eq!(ids.len(), 3);
        assert!(ids[0].starts_with("11111111"));
    }

    #[test]
    fn parses_list_snapshot_with_header_and_blanks() {
        let raw = "\
# Conversations (most recent first)
  11111111-1111-1111-1111-111111111111

22222222-2222-2222-2222-222222222222
";
        let ids = parse_list_snapshot(raw);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[1], "22222222-2222-2222-2222-222222222222");
    }

    #[test]
    fn parses_list_snapshot_empty() {
        let ids = parse_list_snapshot("nothing here\n");
        assert!(ids.is_empty());
    }
}
