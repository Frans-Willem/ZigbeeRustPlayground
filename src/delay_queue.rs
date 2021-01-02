use async_std::future::TimeoutError;
use async_std::task::sleep;
use futures::future;
use futures::future::BoxFuture;
use futures::stream::{FusedStream, Stream};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

pub struct DelayQueue<T> {
    items: Vec<(T, BoxFuture<'static, ()>)>,
    waker: Option<Waker>,
}

impl<T> DelayQueue<T> {
    pub fn new() -> Self {
        DelayQueue {
            items: Vec::new(),
            waker: None,
        }
    }
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
    fn pend<X>(&mut self, cx: &mut Context<'_>) -> Poll<X> {
        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
    pub fn insert(&mut self, value: T, timeout: Duration) {
        let timeout = Box::pin(sleep(timeout));
        self.items.push((value, timeout));
    }

    fn poll_expired(&mut self, cx: &mut Context<'_>) -> Poll<T> {
        for index in (0..self.items.len()) {
            if let Poll::Ready(_) = self.items[index].1.as_mut().poll(cx) {
                let (value, _) = self.items.swap_remove(index);
                return Poll::Ready(value);
            }
        }
        self.pend(cx)
    }
}

impl<T> Stream for DelayQueue<T>
where
    T: Unpin,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::into_inner(self).poll_expired(cx).map(Some)
    }
}

impl<T> FusedStream for DelayQueue<T>
where
    T: Unpin,
{
    fn is_terminated(&self) -> bool {
        false
    }
}

impl<T> DelayQueue<T>
where
    T: PartialEq,
{
    pub fn cancel(&mut self, value: &T) {
        self.items.retain(|(x, _)| x != value)
    }

    pub fn reset(&mut self, value: &T, timeout: Duration) {
        let mut changed = false;
        for (current_value, current_timeout) in self.items.iter_mut() {
            if current_value == value {
                *current_timeout = Box::pin(sleep(timeout));
                changed = true;
            }
        }
        if changed {
            self.wake();
        }
    }
}
