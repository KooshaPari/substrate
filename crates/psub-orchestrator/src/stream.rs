//! Minimal `Stream` adapter over a `Vec` for tests and pre-buffered parses.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;

pub struct EventStream<T> {
    inner: std::vec::IntoIter<T>,
}

impl<T> EventStream<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { inner: items.into_iter() }
    }
}

impl<T> Stream for EventStream<T> {
    type Item = T;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.inner.next())
    }
}
