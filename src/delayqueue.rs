use futures::compat::Compat01As03;
use futures::compat::Stream01CompatExt;
use futures::task::{Context, Poll, Waker};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::{LockResult, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::prelude::Async as Async01;
use tokio::prelude::Stream as Stream01;
use tokio::timer::delay_queue::Expired as TokioDQExpired;
use tokio::timer::delay_queue::Key as TokioDQKey;
use tokio::timer::DelayQueue as TokioDQ;

struct WrappedTokioDQ<T>(Arc<Mutex<TokioDQ<T>>>);

impl<T> WrappedTokioDQ<T> {
    fn new() -> WrappedTokioDQ<T> {
        WrappedTokioDQ(Arc::new(Mutex::new(TokioDQ::new())))
    }

    fn lock(&self) -> LockResult<MutexGuard<TokioDQ<T>>> {
        self.0.lock()
    }
}

impl<T> Clone for WrappedTokioDQ<T> {
    fn clone(&self) -> Self {
        WrappedTokioDQ(self.0.clone())
    }
}

impl<T> Stream01 for WrappedTokioDQ<T> {
    type Item = <TokioDQ<T> as Stream01>::Item;
    type Error = <TokioDQ<T> as Stream01>::Error;

    fn poll(&mut self) -> Result<Async01<Option<Self::Item>>, Self::Error> {
        self.0.lock().unwrap().poll()
    }
}

/**
 * Usage loosely based on tokio::timer::DelayQueue, with the following changes:
 * - Futures 0.3 Stream interface
 * - Stream does not complete until you call 'end', no need for manual task awakening
 */
pub struct DelayQueue<T> {
    inner: WrappedTokioDQ<T>,
    inner_as03: Compat01As03<WrappedTokioDQ<T>>,
    finished: bool,
    waker: Option<Waker>,
}

pub struct Key(Option<TokioDQKey>);

#[derive(Debug)]
pub struct Expired<T>(Option<TokioDQExpired<T>>);

impl<T> Expired<T> {
    pub fn get_ref(&self) -> &T {
        self.0.as_ref().unwrap().get_ref()
    }
}

#[derive(Debug)]
pub enum Error {
    TokioError(tokio::timer::Error),
}

impl<T> DelayQueue<T> {
    pub fn new() -> DelayQueue<T> {
        let inner_wrapped = WrappedTokioDQ::new();
        DelayQueue {
            inner: inner_wrapped.clone(),
            inner_as03: inner_wrapped.compat(),
            finished: false,
            waker: None,
        }
    }

    fn wake_me(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake()
        }
    }

    pub fn insert_at(&mut self, value: T, when: Instant) -> Key {
        if self.finished {
            Key(None)
        } else {
            let retval = Key(Some(self.inner.lock().unwrap().insert_at(value, when)));
            self.wake_me();
            retval
        }
    }

    pub fn insert(&mut self, value: T, timeout: Duration) -> Key {
        if self.finished {
            Key(None)
        } else {
            let retval = Key(Some(self.inner.lock().unwrap().insert(value, timeout)));
            self.wake_me();
            retval
        }
    }

    pub fn remove(&mut self, key: &Key) -> Expired<T> {
        if self.finished {
            Expired(None)
        } else if let Key(Some(inner_key)) = key {
            Expired(Some(self.inner.lock().unwrap().remove(inner_key)))
        } else {
            Expired(None)
        }
    }

    pub fn reset_at(&mut self, key: &Key, when: Instant) {
        if !self.finished {
            if let Key(Some(inner_key)) = key {
                self.inner.lock().unwrap().reset_at(inner_key, when);
                self.wake_me();
            }
        }
    }

    pub fn reset(&mut self, key: &Key, timeout: Duration) {
        if !self.finished {
            if let Key(Some(inner_key)) = key {
                self.inner.lock().unwrap().reset(inner_key, timeout);
                self.wake_me();
            }
        }
    }

    pub fn clear(&mut self) {
        if !self.finished {
            self.inner.lock().unwrap().clear();
            self.wake_me();
        }
    }

    pub fn end(&mut self) {
        self.finished = true;
        self.inner.lock().unwrap().clear();
        self.wake_me();
    }
}

impl<T> Stream for DelayQueue<T> {
    type Item = Result<Expired<T>, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if self.finished {
            Poll::Ready(None)
        } else {
            let mut unpinned = self.get_mut();
            match Pin::new(&mut unpinned.inner_as03).poll_next(cx) {
                Poll::Ready(None) => {
                    unpinned.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
                Poll::Ready(Some(Ok(x))) => Poll::Ready(Some(Ok(Expired(Some(x))))),
                Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(Error::TokioError(e)))),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}

impl<T> Unpin for DelayQueue<T> {}
