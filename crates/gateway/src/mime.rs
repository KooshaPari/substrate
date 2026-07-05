use std::collections::HashMap;

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

pub fn common_types() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    for (ext, mime) in [("html","text/html"),("css","text/css"),("js","application/javascript"),("json","application/json"),("png","image/png"),("jpg","image/jpeg"),("txt","text/plain")] {
        m.insert(ext, mime);
    }
    m
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn html() { assert_eq!(lookup("html"), "text/html"); assert_eq!(lookup("HTML"), "text/html"); }
    #[test] fn json() { assert_eq!(lookup("json"), "application/json"); }
    #[test] fn unknown() { assert_eq!(lookup("xyz"), "application/octet-stream"); }
    #[test] fn png() { assert_eq!(lookup("png"), "image/png"); }
    #[test] fn common_types() { let m = common_types(); assert_eq!(m.get("html").copied(), Some("text/html")); }
}
