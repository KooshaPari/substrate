pub fn parse(s: &str) -> Option<ParsedUrl> {
    let s = s.trim();
    let (scheme, rest) = s.split_once("://")?;
    let (authority, path) = match rest.find('/') {
        Some(pos) => (&rest[..pos], &rest[pos..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => {
            let port: u16 = p.parse().ok()?;
            (h.to_string(), Some(port))
        }
        None => (authority.to_string(), None),
    };
    Some(ParsedUrl {
        scheme: scheme.to_string(),
        host,
        port,
        path: path.to_string(),
    })
}

#[derive(Debug, PartialEq)]
pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
}

pub fn build_url(scheme: &str, host: &str, port: Option<u16>, path: &str) -> String {
    match port {
        Some(p) => format!("{}://{}:{}{}", scheme, host, p, path),
        None => format!("{}://{}{}", scheme, host, path),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_full() {
        let u = parse("https://example.com:8080/path").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, Some(8080));
        assert_eq!(u.path, "/path");
    }
    #[test]
    fn parse_no_port() {
        let u = parse("http://localhost/api").unwrap();
        assert_eq!(u.host, "localhost");
        assert_eq!(u.port, None);
        assert_eq!(u.path, "/api");
    }
    #[test]
    fn parse_no_path() {
        let u = parse("http://x.com").unwrap();
        assert_eq!(u.path, "/");
    }
    #[test]
    fn parse_invalid() {
        assert!(parse("not a url").is_none());
    }
    #[test]
    fn build_with_port() {
        assert_eq!(
            build_url("https", "x.com", Some(443), "/a"),
            "https://x.com:443/a"
        );
    }
    #[test]
    fn build_no_port() {
        assert_eq!(build_url("http", "x.com", None, "/"), "http://x.com/");
    }
}
