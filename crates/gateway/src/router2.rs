use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Route { pub path: String, pub method: String, pub handler: String }
pub struct Router { routes: Vec<Route> }
impl Router {
    pub fn new() -> Self { Self { routes: Vec::new() } }
    pub fn add(&mut self, method: impl Into<String>, path: impl Into<String>, handler: impl Into<String>) {
        self.routes.push(Route { method: method.into(), path: path.into(), handler: handler.into() });
    }
    pub fn match_route(&self, method: &str, path: &str) -> Option<&Route> {
        self.routes.iter().find(|r| r.method == method && r.path == path)
    }
    pub fn match_pattern(&self, method: &str, pattern: &str) -> Vec<&Route> {
        self.routes.iter().filter(|r| r.method == method && path_matches(pattern, &r.path)).collect()
    }
    pub fn route_count(&self) -> usize { self.routes.len() }
    pub fn methods(&self) -> HashMap<String, usize> {
        let mut m = HashMap::new();
        for r in &self.routes { *m.entry(r.method.clone()).or_insert(0) += 1; }
        m
    }
}
fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern == path { return true; }
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() != 2 { return false; }
        path.starts_with(parts[0]) && path.ends_with(parts[1]) && path.len() >= parts[0].len() + parts[1].len()
    } else { false }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn add_match() { let mut r = Router::new(); r.add("GET", "/users", "list_users"); assert_eq!(r.match_route("GET", "/users").unwrap().handler, "list_users"); }
    #[test] fn no_match() { let mut r = Router::new(); r.add("GET", "/users", "x"); assert!(r.match_route("POST", "/users").is_none()); }
    #[test] fn pattern() { let mut r = Router::new(); r.add("GET", "/api/*/users", "h"); assert_eq!(r.match_pattern("GET", "/api/v1/users").len(), 1); }
    #[test] fn count() { let mut r = Router::new(); r.add("GET", "/a", "h1"); r.add("POST", "/a", "h2"); assert_eq!(r.route_count(), 2); }
    #[test] fn methods() { let mut r = Router::new(); r.add("GET", "/a", "h"); r.add("GET", "/b", "h"); r.add("POST", "/c", "h"); assert_eq!(r.methods().get("GET").copied(), Some(2)); }
}
