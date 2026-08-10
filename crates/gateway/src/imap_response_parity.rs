// Minimal IMAP4rev1 response parser.
//
// References:
//   RFC 3501 - INTERNET MESSAGE ACCESS PROTOCOL - VERSION 4rev1
//   - Section 7 (Server Responses): responses fall into three categories:
//       * Untagged responses -- prefixed with `*` (server data/status).
//       * Tagged responses   -- prefixed with the client's tag (command completion result).
//       * Command continuation request -- prefixed with `+` (prompt for more data, e.g. APPEND).
//   - Section 7.1 (Server Responses - Status Responses): status responses are tagged and have
//     the form `<tag> <status> [response code] <human-readable text>` where <status> is one of
//     OK, NO, BAD.
//   - Section 7.2 (Server Responses - Server and Mailbox Status): untagged status responses use
//     the same OK/NO/BAD form, e.g. `* OK [UNSEEN 12] Message 12 is first unseen`.
//   - Section 6 (Client Commands) is the source for command forms like CAPABILITY, LIST, FETCH.
//
// We deliberately stop at a one-pass line parser: enough to classify responses and pull out
// the response code from OK/NO/BAD lines. We do not try to be a complete IMAP parser.

/// The kind of response marker at the start of a line.
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseKind {
    /// `* ...` -- server data or status. Carries the remainder after `* `.
    Untagged,
    /// `<tag> ...` -- command completion result.
    Tagged(String),
    /// `+ ...` -- continuation request (e.g. APPEND literal-data prompt).
    Continuation,
}

/// Standard status responses (RFC 3501 Section 7.1).
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Ok,
    No,
    Bad,
}

/// A parsed OK/NO/BAD response (tagged or untagged).
#[derive(Debug, Clone, PartialEq)]
pub struct StatusResponse {
    pub kind: ResponseKind,
    pub status: Status,
    /// Optional bracketed response code, e.g. `READ-WRITE`, `UNSEEN 12`, `UIDVALIDITY 3857529045`.
    pub response_code: Option<String>,
    /// Human-readable text after the response code. May be empty.
    pub text: String,
}

/// Parsing failure modes.
#[derive(Debug, PartialEq)]
pub enum ImapParseError {
    EmptyLine,
    /// A continuation request that contains non-text after `+` is not modeled -- we treat it as
    /// OK at the caller's discretion.
    BadStructure(String),
}

/// Parse a single IMAP response line (CRLF-stripped).
///
/// Returns the response classification. Lines that do not start with `*`, `+`, or a tag are
/// rejected. The remainder of the line (after the marker) is preserved for further parsing.
pub fn classify_response(line: &str) -> Result<(ResponseKind, &str), ImapParseError> {
    if line.is_empty() {
        return Err(ImapParseError::EmptyLine);
    }
    // Per RFC 3501 grammar, server lines start with "* ", "+ ", or "<tag> ".
    if let Some(rest) = line.strip_prefix("* ") {
        return Ok((ResponseKind::Untagged, rest));
    }
    if let Some(rest) = line.strip_prefix("+ ") {
        return Ok((ResponseKind::Continuation, rest));
    }
    // Tagged response: first whitespace-separated token is the tag.
    if let Some(space_idx) = line.find(' ') {
        let tag = &line[..space_idx];
        // The tag must be non-empty and may NOT be a status word; we accept any non-space
        // token that isn't `*` or `+` as a tag.
        if !tag.is_empty() && tag != "*" && tag != "+" {
            return Ok((
                ResponseKind::Tagged(tag.to_string()),
                &line[space_idx + 1..],
            ));
        }
    }
    Err(ImapParseError::BadStructure(line.to_string()))
}

/// Parse a status response (OK/NO/BAD). Returns `None` if the remainder is not a status line.
pub fn parse_status_response(line: &str) -> Result<Option<StatusResponse>, ImapParseError> {
    let (kind, rest) = classify_response(line)?;
    // The rest must start with OK / NO / BAD (case-insensitive per the formal grammar; the wire
    // is canonical uppercase in practice).
    let (status_word, after_status) = match split_status(rest) {
        Ok(parts) => parts,
        // Non-status untagged responses (e.g. `* 172 EXISTS`) have a numeric/numeric-prefix
        // payload rather than a status word; from the caller's perspective this is "not a
        // status response" rather than a parse error.
        Err(_) => return Ok(None),
    };
    let status = match status_word {
        "OK" => Status::Ok,
        "NO" => Status::No,
        "BAD" => Status::Bad,
        _ => return Ok(None),
    };
    // Optional "[RESPONSE-CODE]" followed by human text. The response code is a single
    // bracketed token -- anything after the closing bracket is text.
    let (response_code, text) = split_response_code(after_status);
    Ok(Some(StatusResponse {
        kind,
        status,
        response_code,
        text: text.to_string(),
    }))
}

/// Convenience: parse a CAPABILITY untagged response. Returns the list of capability words.
pub fn parse_capability(line: &str) -> Result<Vec<String>, ImapParseError> {
    let (kind, rest) = classify_response(line)?;
    if !matches!(kind, ResponseKind::Untagged) {
        return Err(ImapParseError::BadStructure(line.to_string()));
    }
    let after = rest.strip_prefix("CAPABILITY ").ok_or_else(|| {
        ImapParseError::BadStructure(format!("expected CAPABILITY, got: {}", rest))
    })?;
    Ok(after
        .split_ascii_whitespace()
        .map(|s| s.to_string())
        .collect())
}

/// Convenience: parse a LIST untagged response. Returns (attributes, delimiter, name).
/// `delimiter` is the empty string for NIL.
pub fn parse_list(line: &str) -> Result<(Vec<String>, String, String), ImapParseError> {
    let (kind, rest) = classify_response(line)?;
    if !matches!(kind, ResponseKind::Untagged) {
        return Err(ImapParseError::BadStructure(line.to_string()));
    }
    let after = rest
        .strip_prefix("LIST ")
        .ok_or_else(|| ImapParseError::BadStructure(format!("expected LIST, got: {}", rest)))?;
    // After "LIST ", expect "(attrs) delim name" where name may be quoted, NIL, or a literal.
    let (attrs, after_attrs) = read_parenthesized(after)?;
    let mut parts = after_attrs.splitn(2, ' ');
    let delim_token = parts
        .next()
        .ok_or_else(|| ImapParseError::BadStructure(after_attrs.to_string()))?;
    let delim = if delim_token == "NIL" {
        String::new()
    } else if delim_token.starts_with('"') && delim_token.ends_with('"') && delim_token.len() >= 2 {
        delim_token[1..delim_token.len() - 1].to_string()
    } else {
        delim_token.to_string()
    };
    let name = parts
        .next()
        .ok_or_else(|| ImapParseError::BadStructure(after_attrs.to_string()))?
        .to_string();
    Ok((attrs, delim, name))
}

fn split_status(rest: &str) -> Result<(&str, &str), ImapParseError> {
    // status_word may be followed by either ' ' (response code or text) or '[' (response code
    // with no intervening space, per RFC 3501 formal grammar: "OK" SP [resp-text-code] "...").
    let mut last = 0usize;
    for (i, ch) in rest.char_indices() {
        if ch.is_ascii_alphabetic() {
            last = i + ch.len_utf8();
        } else {
            break;
        }
    }
    if last == 0 {
        return Err(ImapParseError::BadStructure(rest.to_string()));
    }
    let word = &rest[..last];
    let after = rest[last..].trim_start();
    Ok((word, after))
}

fn split_response_code(after_status: &str) -> (Option<String>, &str) {
    // "[code]" optional, followed by rest. Strip a leading space.
    let after_status = after_status.trim_start();
    if let Some(stripped) = after_status.strip_prefix('[') {
        if let Some(end_idx) = stripped.find(']') {
            let code = stripped[..end_idx].to_string();
            let text = stripped[end_idx + 1..].trim_start();
            return (Some(code), text);
        }
    }
    (None, after_status)
}

fn read_parenthesized(s: &str) -> Result<(Vec<String>, &str), ImapParseError> {
    let s = s.trim_start();
    let after_open = s
        .strip_prefix('(')
        .ok_or_else(|| ImapParseError::BadStructure(format!("expected '(', got: {}", s)))?;
    // Walk until matching ')'. Attributes may themselves be parenthesized (e.g. (\Noselect \Marked)).
    let mut depth: i32 = 1;
    let mut end = after_open.len();
    for (i, ch) in after_open.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(ImapParseError::BadStructure(format!(
            "unbalanced parens in: {}",
            s
        )));
    }
    let inside = &after_open[..end];
    let after = after_open[end + 1..].trim_start();
    // Inside: whitespace-separated flag tokens. If empty, list is empty.
    let attrs: Vec<String> = if inside.trim().is_empty() {
        Vec::new()
    } else {
        inside
            .split_ascii_whitespace()
            .map(|s| s.to_string())
            .collect()
    };
    Ok((attrs, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_untagged() {
        let (kind, rest) = classify_response("* 172 EXISTS").unwrap();
        assert_eq!(kind, ResponseKind::Untagged);
        assert_eq!(rest, "172 EXISTS");
    }

    #[test]
    fn classify_continuation() {
        let (kind, rest) = classify_response("+ Ready for literal data").unwrap();
        assert_eq!(kind, ResponseKind::Continuation);
        assert_eq!(rest, "Ready for literal data");
    }

    #[test]
    fn classify_tagged() {
        let (kind, rest) = classify_response("A142 OK [READ-WRITE] SELECT completed").unwrap();
        assert_eq!(kind, ResponseKind::Tagged("A142".to_string()));
        assert_eq!(rest, "OK [READ-WRITE] SELECT completed");
    }

    #[test]
    fn classify_rejects_empty() {
        assert_eq!(classify_response(""), Err(ImapParseError::EmptyLine));
    }

    #[test]
    fn classify_rejects_bare_token() {
        // No space -> not a valid tagged response.
        assert!(matches!(
            classify_response("A142"),
            Err(ImapParseError::BadStructure(_))
        ));
    }

    #[test]
    fn parse_ok_with_response_code_and_text() {
        // From RFC 3501 Section 7.2.1 example:
        //   S: * OK [UNSEEN 12] Message 12 is first unseen
        let resp = parse_status_response("* OK [UNSEEN 12] Message 12 is first unseen")
            .unwrap()
            .unwrap();
        assert_eq!(resp.kind, ResponseKind::Untagged);
        assert_eq!(resp.status, Status::Ok);
        assert_eq!(resp.response_code.as_deref(), Some("UNSEEN 12"));
        assert_eq!(resp.text, "Message 12 is first unseen");
    }

    #[test]
    fn parse_tagged_ok_with_read_write_code() {
        // RFC 3501 Section 6.11.1 example:
        //   S: A142 OK [READ-WRITE] SELECT completed
        let resp = parse_status_response("A142 OK [READ-WRITE] SELECT completed")
            .unwrap()
            .unwrap();
        assert_eq!(resp.kind, ResponseKind::Tagged("A142".to_string()));
        assert_eq!(resp.status, Status::Ok);
        assert_eq!(resp.response_code.as_deref(), Some("READ-WRITE"));
        assert_eq!(resp.text, "SELECT completed");
    }

    #[test]
    fn parse_no_response_without_code() {
        // RFC 3501 Section 7.1 example: A684 NO Name "foo" has inferior hierarchical names
        let resp = parse_status_response(r#"A684 NO Name "foo" has inferior hierarchical names"#)
            .unwrap()
            .unwrap();
        assert_eq!(resp.status, Status::No);
        assert_eq!(resp.response_code, None);
        assert!(resp.text.starts_with("Name"));
    }

    #[test]
    fn parse_bad_response() {
        let resp = parse_status_response("B001 BAD Unknown command")
            .unwrap()
            .unwrap();
        assert_eq!(resp.status, Status::Bad);
        assert_eq!(resp.text, "Unknown command");
    }

    #[test]
    fn parse_capability_list() {
        // RFC 3501 Section 6.7.1 example:
        //   S: * CAPABILITY IMAP4rev1 STARTTLS AUTH=GSSAPI LOGINDISABLED
        let caps =
            parse_capability("* CAPABILITY IMAP4rev1 STARTTLS AUTH=GSSAPI LOGINDISABLED").unwrap();
        assert_eq!(
            caps,
            vec![
                "IMAP4rev1".to_string(),
                "STARTTLS".to_string(),
                "AUTH=GSSAPI".to_string(),
                "LOGINDISABLED".to_string(),
            ]
        );
    }

    #[test]
    fn parse_list_with_flags_and_delimiter() {
        // RFC 3501 Section 7.3.1 example:
        //   S: * LIST (\Noselect) "/" foo
        let (attrs, delim, name) = parse_list("* LIST (\\Noselect) \"/\" foo").unwrap();
        assert_eq!(attrs, vec!["\\Noselect".to_string()]);
        assert_eq!(delim, "/");
        assert_eq!(name, "foo");
    }

    #[test]
    fn parse_list_with_nested_parens() {
        // Real-world LIST response with multiple flags and a NIL delimiter.
        let (attrs, delim, name) =
            parse_list("* LIST (\\Marked \\HasChildren) NIL \"INBOX\"").unwrap();
        assert_eq!(
            attrs,
            vec!["\\Marked".to_string(), "\\HasChildren".to_string()]
        );
        assert_eq!(delim, "");
        assert_eq!(name, "\"INBOX\"");
    }

    #[test]
    fn non_status_response_returns_none() {
        // EXISTS is not OK/NO/BAD.
        let resp = parse_status_response("* 172 EXISTS").unwrap();
        assert!(resp.is_none());
    }
}
