#[derive(Debug, PartialEq)]
pub struct Uri {
    pub scheme: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

pub fn parse(input: &str) -> Option<Uri> {
    let mut rest = input;
    let mut scheme = None;
    if let Some(colon) = rest.find(':') {
        let pre = &rest[..colon];
        if !pre.is_empty() && pre.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
            scheme = Some(pre.to_ascii_lowercase());
            rest = &rest[colon + 1..];
        }
    }
    let mut host = None;
    let mut port = None;
    if rest.starts_with("//") {
        rest = &rest[2..];
        let authority_end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        rest = &rest[authority_end..];
        if let Some(at) = authority.rfind('@') {
            // userinfo ignored (defensive)
            let _ = &authority[..at];
        }
        if let Some(colon) = authority.find(':') {
            host = Some(authority[..colon].to_string());
            port = authority[colon + 1..].parse::<u16>().ok();
        } else {
            host = Some(authority.to_string());
        }
    }
    let mut fragment = None;
    if let Some(hash) = rest.find('#') {
        fragment = Some(rest[hash + 1..].to_string());
        rest = &rest[..hash];
    }
    let mut query = None;
    let path = if let Some(q) = rest.find('?') {
        let p = rest[..q].to_string();
        query = Some(rest[q + 1..].to_string());
        p
    } else {
        rest.to_string()
    };
    Some(Uri { scheme, host, port, path, query, fragment })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn full() {
        let u = parse("https://example.com:8080/path?x=1#frag").unwrap();
        assert_eq!(u.scheme.as_deref(), Some("https"));
        assert_eq!(u.host.as_deref(), Some("example.com"));
        assert_eq!(u.port, Some(8080));
        assert_eq!(u.path, "/path");
        assert_eq!(u.query.as_deref(), Some("x=1"));
        assert_eq!(u.fragment.as_deref(), Some("frag"));
    }
    #[test] fn no_scheme() {
        let u = parse("/just/a/path?ok=1").unwrap();
        assert_eq!(u.scheme, None);
        assert_eq!(u.host, None);
        assert_eq!(u.path, "/just/a/path");
        assert_eq!(u.query.as_deref(), Some("ok=1"));
    }
    #[test] fn path_only() {
        let u = parse("/hello").unwrap();
        assert_eq!(u.path, "/hello");
        assert_eq!(u.query, None);
    }
    #[test] fn scheme_lower() {
        let u = parse("HTTP://Example.COM/").unwrap();
        assert_eq!(u.scheme.as_deref(), Some("http"));
        assert_eq!(u.host.as_deref(), Some("Example.COM"));
    }
    #[test] fn fragment_only() {
        let u = parse("foo#bar").unwrap();
        assert_eq!(u.fragment.as_deref(), Some("bar"));
        assert_eq!(u.path, "foo");
    }
    #[test] fn no_port() {
        let u = parse("https://x.com/").unwrap();
        assert_eq!(u.host.as_deref(), Some("x.com"));
        assert_eq!(u.port, None);
    }
}
