// Minimal W3C WebVTT cue parser.
//
// References:
//   W3C WebVTT spec: https://www.w3.org/TR/webvtt1/
//   File body grammar (section 3 "WebVTT files"):
//     - first line must be the signature: `WEBVTT` optionally followed by U+0020 SPACE
//       or U+0009 TAB and any text up to the line terminator.
//     - the byte-order mark (U+FEFF) may precede the file body.
//     - cue blocks contain an optional identifier line, a timing line, and one or
//       more payload lines.
//     - timing line: `start --> end` with timestamps `HH:MM:SS.MMM` or `MM:SS.MMM`.
//       An optional settings list follows `end`, separated by U+0020 SPACE.
//     - NOTE blocks (single or multi-line) are skipped.
//     - STYLE and REGION blocks are recognised and skipped (parser is cue-only).
//
// This parser is intentionally minimal: it does not implement cue text
// internal-token parsing, only the cue-block structure, timings, and the
// raw settings list. Returns `Err(String)` on malformed input.

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub id: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub settings: String,
    pub payload: String,
}

fn ts_to_ms(ts: &str) -> Result<u64, String> {
    // Accepts "HH:MM:SS.MMM" or "MM:SS.MMM". Hours optional.
    let (h, rest) = if ts.bytes().filter(|&b| b == b':').count() == 2 {
        let mut split = ts.splitn(2, ':');
        let h = split.next().ok_or_else(|| format!("bad ts: {ts}"))?;
        let rest = split.next().ok_or_else(|| format!("bad ts: {ts}"))?;
        (h, rest)
    } else {
        ("0", ts)
    };
    let mut parts = rest.splitn(2, ':');
    let m = parts
        .next()
        .ok_or_else(|| format!("bad ts: {ts}"))?
        .parse::<u64>()
        .map_err(|e| format!("bad minutes in {ts}: {e}"))?;
    let sm = parts
        .next()
        .ok_or_else(|| format!("bad ts: {ts}"))?
        .splitn(2, '.');
    let mut sm_iter = sm;
    let s = sm_iter
        .next()
        .ok_or_else(|| format!("bad ts: {ts}"))?
        .parse::<u64>()
        .map_err(|e| format!("bad seconds in {ts}: {e}"))?;
    let frac_str = sm_iter.next().unwrap_or("0");
    // Pad/truncate fraction to 3 digits (ms).
    let frac_ms: u64 = if frac_str.is_empty() {
        0
    } else {
        let mut f = frac_str.to_string();
        while f.len() < 3 {
            f.push('0');
        }
        f.truncate(3);
        f.parse::<u64>()
            .map_err(|e| format!("bad ms in {ts}: {e}"))?
    };
    let hours: u64 = h.parse().map_err(|e| format!("bad hours in {ts}: {e}"))?;
    Ok(((hours * 3600 + m * 60 + s) * 1000) + frac_ms)
}

fn strip_bom(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix('\u{FEFF}') {
        rest
    } else {
        s
    }
}

/// Parse a WebVTT file body into cues. Strips an optional UTF-8 BOM.
/// Returns `Err(String)` if the signature line is missing or a cue timing
/// line is malformed.
pub fn parse(input: &str) -> Result<Vec<Cue>, String> {
    let body = strip_bom(input);
    // Normalise line endings: split on either CRLF or LF.
    let raw_lines: Vec<&str> = body.lines().collect();
    if raw_lines.is_empty() {
        return Err("empty WebVTT body".into());
    }
    let sig = raw_lines[0].trim_end_matches('\r');
    if sig == "WEBVTT" || sig.starts_with("WEBVTT ") || sig.starts_with("WEBVTT\t") {
        // ok
    } else {
        return Err(format!(
            "missing WEBVTT signature (first line was: {sig:?})"
        ));
    }

    let mut cues: Vec<Cue> = Vec::new();
    let mut i = 1usize;
    while i < raw_lines.len() {
        // Skip blank lines.
        while i < raw_lines.len() && raw_lines[i].trim_end_matches('\r').is_empty() {
            i += 1;
        }
        if i >= raw_lines.len() {
            break;
        }
        // Detect block start: a line containing "-->" is a cue timing line.
        // Lines beginning with `NOTE`, `STYLE`, or `REGION` start a non-cue
        // block that we skip until the next blank line.
        let first = raw_lines[i].trim_end_matches('\r');
        let block_keyword = if first.starts_with("NOTE") {
            Some("NOTE")
        } else if first.starts_with("STYLE") {
            Some("STYLE")
        } else if first.starts_with("REGION") {
            Some("REGION")
        } else {
            None
        };
        if first.contains("-->") {
            // No identifier line: this line IS the timing line.
            let id: Option<String> = None;
            let timing_line = first.to_string();
            i += 1;
            // Payload: read until blank line or EOF.
            let mut payload_lines: Vec<String> = Vec::new();
            while i < raw_lines.len() && !raw_lines[i].trim_end_matches('\r').is_empty() {
                payload_lines.push(raw_lines[i].trim_end_matches('\r').to_string());
                i += 1;
            }
            let payload = payload_lines.join("\n");
            let (start_ms, end_ms, settings) = parse_timing(&timing_line)?;
            cues.push(Cue {
                id,
                start_ms,
                end_ms,
                settings,
                payload,
            });
        } else if block_keyword.is_some() {
            // Skip the entire non-cue block (NOTE / STYLE / REGION) until the
            // next blank line.
            i += 1;
            while i < raw_lines.len() && !raw_lines[i].trim_end_matches('\r').is_empty() {
                i += 1;
            }
        } else {
            // Identifier line: read it, then the next line must contain the
            // timing arrow.
            let id_line = first.to_string();
            if i + 1 >= raw_lines.len() {
                return Err(format!("cue id without timing line: {id_line:?}"));
            }
            let timing_line = raw_lines[i + 1].trim_end_matches('\r').to_string();
            if !timing_line.contains("-->") {
                return Err(format!(
                    "expected timing line after cue id, got: {timing_line:?}"
                ));
            }
            i += 2;
            let mut payload_lines: Vec<String> = Vec::new();
            while i < raw_lines.len() && !raw_lines[i].trim_end_matches('\r').is_empty() {
                payload_lines.push(raw_lines[i].trim_end_matches('\r').to_string());
                i += 1;
            }
            let payload = payload_lines.join("\n");
            let (start_ms, end_ms, settings) = parse_timing(&timing_line)?;
            cues.push(Cue {
                id: Some(id_line),
                start_ms,
                end_ms,
                settings,
                payload,
            });
        }
    }

    Ok(cues)
}

fn parse_timing(line: &str) -> Result<(u64, u64, String), String> {
    // The arrow must be surrounded by whitespace per spec: "start --> end".
    let arrow_idx = line
        .find("-->")
        .ok_or_else(|| format!("timing line missing arrow: {line:?}"))?;
    // Walk back to find start of "end", respecting whitespace.
    let after_arrow = &line[arrow_idx + 3..];
    let end_start_rel = after_arrow
        .chars()
        .position(|c| !c.is_whitespace())
        .ok_or_else(|| format!("timing line missing end: {line:?}"))?;
    let after_end = &after_arrow[end_start_rel..];
    // End ts terminates at whitespace or EOL.
    let end_ts_len = after_end
        .chars()
        .take_while(|c| !c.is_whitespace())
        .map(|c| c.len_utf8())
        .sum::<usize>();
    let end_ts = &after_end[..end_ts_len];
    // Settings: any remaining text after end ts.
    let settings = after_end[end_ts_len..].trim().to_string();
    // Walk back from arrow to find end of "start" ts.
    let before_arrow = &line[..arrow_idx];
    let start_end_rel = before_arrow
        .chars()
        .rev()
        .take_while(|c| !c.is_whitespace())
        .map(|c| c.len_utf8())
        .sum::<usize>();
    let start_ts = &before_arrow[..before_arrow.len() - start_end_rel];
    let start_ms = ts_to_ms(start_ts.trim())?;
    let end_ms = ts_to_ms(end_ts)?;
    if end_ms < start_ms {
        return Err(format!("end before start: {line:?}"));
    }
    Ok((start_ms, end_ms, settings))
}

/// Render cues back to a WebVTT file body (round-trip helper).
pub fn write(cues: &[Cue]) -> String {
    let mut out = String::new();
    out.push_str("WEBVTT\n\n");
    for c in cues {
        if let Some(id) = &c.id {
            out.push_str(id);
            out.push('\n');
        }
        out.push_str(&format_timestamp(c.start_ms));
        out.push_str(" --> ");
        out.push_str(&format_timestamp(c.end_ms));
        if !c.settings.is_empty() {
            out.push(' ');
            out.push_str(&c.settings);
        }
        out.push('\n');
        if !c.payload.is_empty() {
            out.push_str(&c.payload);
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

fn format_timestamp(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_only() {
        let cues = parse("WEBVTT\n").unwrap();
        assert!(cues.is_empty());
    }

    #[test]
    fn signature_with_header_text() {
        let cues = parse("WEBVTT - Some title\n\n").unwrap();
        assert!(cues.is_empty());
    }

    #[test]
    fn bom_is_stripped() {
        let input = "\u{FEFF}WEBVTT\n\n00:00:01.000 --> 00:00:02.500\nHello\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].end_ms, 2500);
        assert_eq!(cues[0].payload, "Hello");
    }

    #[test]
    fn basic_cue_no_id() {
        let input = "WEBVTT\n\n00:00:01.000 --> 00:00:02.500\nHello world\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].id, None);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].end_ms, 2500);
        assert_eq!(cues[0].payload, "Hello world");
        assert_eq!(cues[0].settings, "");
    }

    #[test]
    fn cue_with_identifier() {
        let input = "WEBVTT\n\nintro-1\n00:00:00.500 --> 00:00:03.000\nHi there\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].id.as_deref(), Some("intro-1"));
        assert_eq!(cues[0].start_ms, 500);
        assert_eq!(cues[0].end_ms, 3000);
    }

    #[test]
    fn cue_with_settings() {
        let input =
            "WEBVTT\n\n00:00:00.000 --> 00:00:05.000 align:start line:50%\nBottom-aligned text\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].settings, "align:start line:50%");
        assert_eq!(cues[0].payload, "Bottom-aligned text");
    }

    #[test]
    fn cue_short_timestamp_form() {
        // MM:SS.MMM form (no hours) is also legal per spec.
        let input = "WEBVTT\n\n01:30.000 --> 02:00.000\nShort form\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 90_000);
        assert_eq!(cues[0].end_ms, 120_000);
    }

    #[test]
    fn note_block_is_skipped() {
        let input =
            "WEBVTT\n\nNOTE this is a single-line note\n\n00:00:00.000 --> 00:00:01.000\nCue\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].payload, "Cue");
    }

    #[test]
    fn style_block_is_skipped() {
        let input = "WEBVTT\n\nSTYLE\n::cue { color: red }\n\n00:00:00.000 --> 00:00:01.000\nCue\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 1);
    }

    #[test]
    fn multiple_cues() {
        let input = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nfirst\n\n00:00:02.000 --> 00:00:03.000\nsecond\n";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].payload, "first");
        assert_eq!(cues[1].start_ms, 2000);
        assert_eq!(cues[1].payload, "second");
    }

    #[test]
    fn missing_signature_errors() {
        let err = parse("not a vtt file\n").unwrap_err();
        assert!(err.contains("signature"));
    }

    #[test]
    fn malformed_timing_errors() {
        let input = "WEBVTT\n\n00:00:00.000 -\\-> 00:00:01.000\nBad\n";
        assert!(parse(input).is_err());
    }

    #[test]
    fn end_before_start_errors() {
        let input = "WEBVTT\n\n00:00:02.000 --> 00:00:01.000\nInverted\n";
        assert!(parse(input).is_err());
    }

    #[test]
    fn round_trip_preserves_cues() {
        let original = vec![
            Cue {
                id: Some("c1".into()),
                start_ms: 1000,
                end_ms: 2500,
                settings: "align:start".into(),
                payload: "Hello".into(),
            },
            Cue {
                id: None,
                start_ms: 3000,
                end_ms: 4500,
                settings: String::new(),
                payload: "World".into(),
            },
        ];
        let out = write(&original);
        let reparsed = parse(&out).unwrap();
        assert_eq!(reparsed.len(), 2);
        assert_eq!(reparsed[0].id.as_deref(), Some("c1"));
        assert_eq!(reparsed[0].start_ms, 1000);
        assert_eq!(reparsed[0].end_ms, 2500);
        assert_eq!(reparsed[0].settings, "align:start");
        assert_eq!(reparsed[0].payload, "Hello");
        assert_eq!(reparsed[1].start_ms, 3000);
        assert_eq!(reparsed[1].payload, "World");
    }
}
