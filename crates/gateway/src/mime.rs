pub fn lookup(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "md" => "text/markdown",
        _ => "application/octet-stream",
    }
}

pub const COMMON_TYPES: &[(&str, &str)] = &[
    ("html", "text/html"),
    ("css", "text/css"),
    ("js", "application/javascript"),
    ("json", "application/json"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("txt", "text/plain"),
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html() {
        assert_eq!(lookup("html"), "text/html");
        assert_eq!(lookup("HTML"), "text/html");
    }
    #[test]
    fn json() {
        assert_eq!(lookup("json"), "application/json");
    }
    #[test]
    fn unknown() {
        assert_eq!(lookup("xyz"), "application/octet-stream");
    }
    #[test]
    fn png() {
        assert_eq!(lookup("png"), "image/png");
    }
    #[test]
    fn common() {
        assert!(COMMON_TYPES
            .iter()
            .any(|(e, m)| *e == "html" && *m == "text/html"));
    }
}
