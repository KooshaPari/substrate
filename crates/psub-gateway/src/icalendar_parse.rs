// Minimal iCalendar (RFC 5545) parser/serializer.
//
// Supports VCALENDAR wrappers containing one or more VEVENT blocks.
// Handles line-folding (CRLF + linear-white-space), simple properties
// (UID, DTSTART, DTEND, SUMMARY, DESCRIPTION, LOCATION), and DATE vs
// DATE-TIME values. TZID parameters are extracted alongside DTSTART/DTEND.

#[derive(Debug, Clone, PartialEq)]
pub struct VEvent {
    pub uid: String,
    pub dtstart: String,
    pub dtend: String,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub tzid: Option<String>,
    pub all_day: bool,
}

impl VEvent {
    pub fn new(uid: impl Into<String>) -> Self {
        Self {
            uid: uid.into(),
            dtstart: String::new(),
            dtend: String::new(),
            summary: String::new(),
            description: String::new(),
            location: String::new(),
            tzid: None,
            all_day: false,
        }
    }
}

pub fn parse(input: &str) -> Result<Vec<VEvent>, String> {
    let lines = unfold(input);
    let mut events = Vec::new();
    let mut in_calendar = false;
    let mut in_event = false;
    let mut current = VEvent::new("");
    for line in &lines {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            continue;
        }
        let (name_with_params, value) = match split_property(trimmed) {
            Some(v) => v,
            None => continue,
        };
        let (name, params) = split_name_params(name_with_params);
        match (name.as_str(), in_event, in_calendar) {
            ("BEGIN", false, false) if value == "VCALENDAR" => {
                in_calendar = true;
            }
            ("END", false, true) if value == "VCALENDAR" => {
                in_calendar = false;
            }
            ("BEGIN", false, true) if value == "VEVENT" => {
                in_event = true;
                current = VEvent::new("");
            }
            ("END", true, true) if value == "VEVENT" => {
                events.push(std::mem::replace(&mut current, VEvent::new("")));
                in_event = false;
            }
            (n, true, _) => {
                apply_property(&mut current, n, &params, value);
            }
            _ => {}
        }
    }
    if in_event {
        return Err("unterminated VEVENT block".into());
    }
    Ok(events)
}

pub fn write(events: &[VEvent]) -> String {
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\n");
    out.push_str("VERSION:2.0\r\n");
    out.push_str("PRODID:-//substrate//icalendar_parse//EN\r\n");
    for ev in events {
        out.push_str("BEGIN:VEVENT\r\n");
        if !ev.uid.is_empty() {
            out.push_str(&format!("UID:{}\r\n", escape_text(&ev.uid)));
        }
        if !ev.dtstart.is_empty() {
            out.push_str(&format!("DTSTART{}:{}\r\n", tzid_param(ev), ev.dtstart));
        }
        if !ev.dtend.is_empty() {
            out.push_str(&format!("DTEND{}:{}\r\n", tzid_param(ev), ev.dtend));
        }
        if !ev.summary.is_empty() {
            out.push_str(&format!("SUMMARY:{}\r\n", escape_text(&ev.summary)));
        }
        if !ev.description.is_empty() {
            for line in fold(&escape_text(&ev.description)) {
                out.push_str(&format!("DESCRIPTION:{}\r\n", line));
            }
        }
        if !ev.location.is_empty() {
            out.push_str(&format!("LOCATION:{}\r\n", escape_text(&ev.location)));
        }
        out.push_str("END:VEVENT\r\n");
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

fn apply_property(ev: &mut VEvent, name: &str, params: &[(String, String)], value: &str) {
    if let Some((_, tz)) = params.iter().find(|(k, _)| k.eq_ignore_ascii_case("TZID")) {
        ev.tzid = Some(tz.clone());
    }
    match name {
        "UID" => ev.uid = unescape_text(value).to_string(),
        "DTSTART" => {
            ev.dtstart = value.to_string();
            ev.all_day = !value.contains('T');
        }
        "DTEND" => {
            ev.dtend = value.to_string();
        }
        "SUMMARY" => ev.summary = unescape_text(value).to_string(),
        "DESCRIPTION" => ev.description = unescape_text(value).to_string(),
        "LOCATION" => ev.location = unescape_text(value).to_string(),
        _ => {}
    }
}

fn tzid_param(ev: &VEvent) -> String {
    match &ev.tzid {
        Some(tz) => format!(";TZID={}", tz),
        None => String::new(),
    }
}

fn split_property(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(':')?;
    Some((&line[..idx], &line[idx + 1..]))
}

fn split_name_params(name_with_params: &str) -> (String, Vec<(String, String)>) {
    let mut parts = name_with_params.split(';');
    let name = parts.next().unwrap_or("").to_ascii_uppercase();
    let mut params = Vec::new();
    for p in parts {
        if let Some(eq) = p.find('=') {
            params.push((p[..eq].to_ascii_uppercase(), p[eq + 1..].to_string()));
        }
    }
    (name, params)
}

fn unfold(input: &str) -> Vec<String> {
    let normalized = input.replace("\r\n", "\n");
    let mut out: Vec<String> = Vec::new();
    for line in normalized.split('\n') {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.push_str(&line[1..]);
                continue;
            }
        }
        out.push(line.to_string());
    }
    out
}

fn fold(text: &str) -> Vec<String> {
    const LIMIT: usize = 75;
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        let char_len = ch.len_utf8();
        if current.len() + char_len > LIMIT {
            out.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out
}

fn unescape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(';') => out.push(';'),
                Some(',') => out.push(','),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_event_parse() {
        let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:abc-1\r\nDTSTART:20260101T100000Z\r\nDTEND:20260101T110000Z\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse(ics).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "abc-1");
        assert_eq!(events[0].summary, "Standup");
        assert_eq!(events[0].dtstart, "20260101T100000Z");
        assert!(!events[0].all_day);
    }

    #[test]
    fn multiple_events() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART:20260101T100000Z\r\nSUMMARY:A\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nDTSTART:20260102T100000Z\r\nSUMMARY:B\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse(ics).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].uid, "a");
        assert_eq!(events[1].uid, "b");
    }

    #[test]
    fn line_folding_round_trip() {
        let original = "This is a long summary that should be folded across multiple lines to comply with RFC 5545 limits on a single content line.";
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:fold\r\nDTSTART:20260101T100000Z\r\nSUMMARY:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            original
        );
        let folded = write(&parse(&ics).unwrap());
        let events = parse(&folded).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, original);
    }

    #[test]
    fn date_vs_date_time() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:all-day\r\nDTSTART;VALUE=DATE:20260101\r\nDTEND;VALUE=DATE:20260102\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse(ics).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].all_day);
        assert_eq!(events[0].dtstart, "20260101");
    }

    #[test]
    fn rejects_malformed_missing_end() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\nSUMMARY:X\r\nEND:VCALENDAR\r\n";
        assert!(parse(ics).is_err());
    }

    #[test]
    fn escapes_commas_semicolons_in_text() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:esc\r\nDTSTART:20260101T100000Z\r\nSUMMARY:Hello, world; with:colon\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse(ics).unwrap();
        assert_eq!(events[0].summary, "Hello, world; with:colon");
    }

    #[test]
    fn parses_tzid() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:tz\r\nDTSTART;TZID=America/Los_Angeles:20260101T100000\r\nDTEND;TZID=America/Los_Angeles:20260101T110000\r\nSUMMARY:Local\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse(ics).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tzid.as_deref(), Some("America/Los_Angeles"));
    }
}
