pub struct M3uEntry {
    pub duration_secs: Option<f64>,
    pub title: Option<String>,
    pub uri: String,
}
pub fn parse(input: &str) -> Vec<M3uEntry> {
    let mut out = Vec::new();
    let mut current = M3uEntry {
        duration_secs: None,
        title: None,
        uri: String::new(),
    };
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#EXTM3U") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            if let Some(comma) = rest.find(',') {
                let dur_str = &rest[..comma];
                current.duration_secs = dur_str.parse::<f64>().ok();
                current.title = Some(rest[comma + 1..].to_string());
            } else {
                current.duration_secs = rest.parse::<f64>().ok();
                current.title = None;
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        current.uri = line.to_string();
        out.push(M3uEntry {
            duration_secs: current.duration_secs.take(),
            title: current.title.take(),
            uri: current.uri.clone(),
        });
    }
    out
}
pub fn render(entries: &[M3uEntry]) -> String {
    let mut out = String::new();
    out.push_str("#EXTM3U\n");
    for e in entries {
        let dur = e
            .duration_secs
            .map(|d| format!("{}", d))
            .unwrap_or_else(|| "-1".into());
        let title = e.title.clone().unwrap_or_default();
        out.push_str(&format!("#EXTINF:{},{}\n", dur, title));
        out.push_str(&format!("{}\n", e.uri));
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_basic() {
        let m = parse("#EXTM3U\n#EXTINF:120,Hello\nhttp://example.com/stream.mp3\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].duration_secs, Some(120.0));
        assert_eq!(m[0].title.as_deref(), Some("Hello"));
        assert_eq!(m[0].uri, "http://example.com/stream.mp3");
    }
    #[test]
    fn parse_no_extm3u_still_works() {
        let m = parse("#EXTINF:120,Hello\nhttp://example.com/stream.mp3\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].title.as_deref(), Some("Hello"));
    }
    #[test]
    fn parse_no_extinf() {
        let m = parse("#EXTM3U\nhttp://example.com/stream.mp3\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].uri, "http://example.com/stream.mp3");
        assert_eq!(m[0].duration_secs, None);
    }
    #[test]
    fn parse_multi() {
        let m = parse("#EXTM3U\n#EXTINF:60,A\nhttp://a\n#EXTINF:120,B\nhttp://b\n");
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].title.as_deref(), Some("B"));
    }
    #[test]
    fn parse_comments_ignored() {
        let m = parse("#EXTM3U\n# some comment\n#EXTINF:60,A\nhttp://a\n");
        assert_eq!(m.len(), 1);
    }
    #[test]
    fn parse_fractional_duration() {
        let m = parse("#EXTM3U\n#EXTINF:120.5,A\nhttp://a\n");
        assert_eq!(m[0].duration_secs, Some(120.5));
    }
    #[test]
    fn parse_empty() {
        let m = parse("");
        assert_eq!(m.len(), 0);
    }
    #[test]
    fn render_roundtrip() {
        let entries = vec![
            M3uEntry {
                duration_secs: Some(60.0),
                title: Some("A".into()),
                uri: "http://a".into(),
            },
            M3uEntry {
                duration_secs: None,
                title: None,
                uri: "http://b".into(),
            },
        ];
        let rendered = render(&entries);
        let parsed = parse(&rendered);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].uri, "http://a");
        assert_eq!(parsed[1].uri, "http://b");
    }
}
