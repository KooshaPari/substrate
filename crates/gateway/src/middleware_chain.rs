use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub trait Middleware: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn handle(&self, req: String, next: Arc<dyn Middleware>) -> BoxFuture<String>;
}

pub struct NoopMiddleware;
impl Middleware for NoopMiddleware {
    fn name(&self) -> &str {
        "noop"
    }
    fn handle(&self, req: String, _next: Arc<dyn Middleware>) -> BoxFuture<String> {
        Box::pin(async move { req })
    }
}

pub struct TimingMiddleware;
impl Middleware for TimingMiddleware {
    fn name(&self) -> &str {
        "timing"
    }
    fn handle(&self, req: String, _next: Arc<dyn Middleware>) -> BoxFuture<String> {
        Box::pin(async move { format!("[timed]{}", req) })
    }
}

pub struct MiddlewareStack {
    items: Vec<Arc<dyn Middleware>>,
}
impl MiddlewareStack {
    pub fn new() -> Self {
        Self { items: vec![] }
    }
    pub fn push(mut self, m: impl Middleware) -> Self {
        self.items.push(Arc::new(m));
        self
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub async fn run(&self, req: String) -> String {
        if self.items.is_empty() {
            return req;
        }
        self.items[0].handle(req, Arc::new(NoopMiddleware)).await
    }
}
impl Default for MiddlewareStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn empty_passthrough() {
        assert_eq!(MiddlewareStack::new().run("x".into()).await, "x");
    }
    #[tokio::test]
    async fn timing_wraps() {
        let r = MiddlewareStack::new()
            .push(TimingMiddleware)
            .run("req".into())
            .await;
        assert!(r.starts_with("[timed]"));
    }
    #[test]
    fn stack_len() {
        assert_eq!(MiddlewareStack::new().push(TimingMiddleware).len(), 1);
    }
    #[test]
    fn empty_is_empty() {
        assert!(MiddlewareStack::new().is_empty());
    }
    #[test]
    fn noop_name() {
        assert_eq!(NoopMiddleware.name(), "noop");
    }
}
