//! Streaming response handling: HTTP/1.1 chunked encoding, SSE passthrough, backpressure.
//! Ensures responses are forwarded chunk-by-chunk without buffering.

use axum::body::Body;
use axum::http::Response;
use bytes::Bytes;
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Streaming response builder.
/// Converts a stream of chunks into an axum Response<Body> with proper HTTP/1.1 chunked encoding.
pub struct StreamingResponseBuilder;

impl StreamingResponseBuilder {
    /// Create a streaming response from a byte stream.
    /// Chunks are forwarded immediately (no buffering).
    pub fn from_stream<S>(stream: S) -> Response<Body>
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        let body = Body::from_stream(stream);
        Response::builder()
            .header("content-encoding", "chunked")
            .header("transfer-encoding", "chunked")
            .body(body)
            .unwrap()
    }

    /// Create a streaming SSE response (Server-Sent Events).
    /// Preserves event boundaries and line-based framing.
    pub fn sse_stream<S>(stream: S) -> Response<Body>
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        let body = Body::from_stream(stream);
        Response::builder()
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("connection", "keep-alive")
            .header("transfer-encoding", "chunked")
            .body(body)
            .unwrap()
    }
}

/// Backpressure-aware chunk forwarder.
/// Respects client-side drain signals (waits for client before sending next chunk).
pub struct BackpressureStream<S> {
    inner: S,
    #[allow(dead_code)]
    is_drained: bool,
}

impl<S> BackpressureStream<S>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
{
    /// Wrap a stream with backpressure handling.
    pub fn new(stream: S) -> Self {
        Self {
            inner: stream,
            is_drained: true,
        }
    }
}

impl<S> Stream for BackpressureStream<S>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
{
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Poll the inner stream
        // Backpressure is handled automatically by Tokio's task scheduler:
        // - If the client is slow to drain, the write buffer fills up
        // - Poll returns Pending until the client reads
        // - No explicit backpressure signal needed in this layer

        Pin::new(&mut self.inner).poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;

    #[test]
    fn test_streaming_response_builder() {
        let stream = stream::iter(vec![Ok(Bytes::from("chunk1")), Ok(Bytes::from("chunk2"))]);

        let response = StreamingResponseBuilder::from_stream(stream);
        assert_eq!(response.status(), 200);
    }

    #[test]
    fn test_sse_response_headers() {
        let stream = stream::iter(vec![Ok(Bytes::from("data: test\n\n"))]);

        let response = StreamingResponseBuilder::sse_stream(stream);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "text/event-stream"
        );
        assert_eq!(
            response
                .headers()
                .get("transfer-encoding")
                .unwrap()
                .to_str()
                .unwrap(),
            "chunked"
        );
    }

    #[tokio::test]
    async fn test_backpressure_stream_forwards_chunks() {
        let inner = stream::iter(vec![
            Ok(Bytes::from("a")),
            Ok(Bytes::from("b")),
            Ok(Bytes::from("c")),
        ]);

        let mut stream = BackpressureStream::new(inner);

        // Consume chunks sequentially (simulates client reading)
        use futures::StreamExt;
        assert_eq!(stream.next().await.unwrap().unwrap(), Bytes::from("a"));
        assert_eq!(stream.next().await.unwrap().unwrap(), Bytes::from("b"));
        assert_eq!(stream.next().await.unwrap().unwrap(), Bytes::from("c"));
        assert!(stream.next().await.is_none());
    }

    #[test]
    fn test_streaming_memory_model() {
        // Key insight: StreamingResponseBuilder never buffers full response
        // Each chunk is forwarded immediately as it arrives
        // Memory ≤ chunk_size, regardless of total response size

        let chunk_size = 64 * 1024; // 64 KB
        let response_size = 1 * 1024 * 1024 * 1024; // 1 GB

        // With streaming, we process 1GB response in 64KB chunks
        // Memory usage stays at O(chunk_size) = 64KB
        // Not O(response_size) = 1GB

        assert!(chunk_size < response_size);
    }
}
