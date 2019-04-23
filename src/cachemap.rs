use futures::{Async, Future, Poll, Stream};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::timer::delay_queue::Key as DelayQueueKey;
use tokio::timer::DelayQueue;

#[derive(Debug)]
pub enum Error {
    TimerError(tokio::timer::Error),
}
impl From<tokio::timer::Error> for Error {
    fn from(timer_err: tokio::timer::Error) -> Error {
        Error::TimerError(timer_err)
    }
}

struct CacheMapInternal<K, V> {
    entries: HashMap<K, (V, DelayQueueKey)>,
    expirations: DelayQueue<K>,
}

impl<K, V> CacheMapInternal<K, V>
where
    K: std::hash::Hash,
    K: std::cmp::Eq,
    K: Clone,
    V: Clone,
{
    fn new() -> CacheMapInternal<K, V> {
        CacheMapInternal {
            entries: HashMap::new(),
            expirations: DelayQueue::new(),
        }
    }

    fn insert(&mut self, key: K, value: V, ttl: Duration) -> Option<V> {
        let delay = self.expirations.insert(key.clone(), ttl);
        if let Some((old_value, old_delay)) = self.entries.insert(key, (value, delay)) {
            self.expirations.remove(&old_delay);
            Some(old_value)
        } else {
            None
        }
    }

    fn get(&mut self, key: K) -> Option<V> {
        self.entries.get(&key).map(|(value, _)| value.clone())
    }

    fn remove(&mut self, key: K) -> Option<V> {
        self.entries.remove(&key).map(|(value, delay)| {
            self.expirations.remove(&delay);
            value
        })
    }

    fn poll(&mut self) -> Poll<(), Error> {
        while let Some(entry) = try_ready!(self.expirations.poll()) {
            self.entries.remove(entry.get_ref());
        }
        Ok(Async::Ready(()))
    }
}

impl<K, V> CacheMapInternal<K, V> {
    fn user_dropped(&mut self) {
        self.expirations.clear()
    }
}

pub struct CacheMap<K, V> {
    inner: Rc<Mutex<CacheMapInternal<K, V>>>,
}

struct CacheMapFuture<K, V>(Rc<Mutex<CacheMapInternal<K, V>>>);

impl<K, V> Future for CacheMapFuture<K, V> {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        unimplemented!()
    }
}

impl<K, V> CacheMap<K, V>
where
    K: std::hash::Hash,
    K: std::cmp::Eq,
    K: std::clone::Clone,
    V: std::clone::Clone,
    K: 'static,
    V: 'static,
{
    pub fn new(handle: tokio_core::reactor::Handle) -> Self {
        let inner = CacheMapInternal::new();
        let inner = Rc::new(Mutex::new(inner));
        handle
            .spawn(CacheMapFuture(inner.clone()).map_err(|e| eprintln!("Error CacheMap: {:?}", e)));
        CacheMap { inner }
    }
}

impl<K, V> CacheMap<K, V>
where
    K: std::hash::Hash,
    K: std::cmp::Eq,
    K: std::clone::Clone,
    V: std::clone::Clone,
{
    pub fn insert(&self, key: K, value: V, ttl: Duration) -> Option<V> {
        self.inner.lock().unwrap().insert(key, value, ttl)
    }
    pub fn get(&self, key: K) -> Option<V> {
        self.inner.lock().unwrap().get(key)
    }
    pub fn remove(&self, key: K) -> Option<V> {
        self.inner.lock().unwrap().remove(key)
    }
}

impl<K, V> Drop for CacheMap<K, V> {
    fn drop(&mut self) {
        let mut guard = self.inner.lock().unwrap();
        guard.user_dropped()
    }
}
