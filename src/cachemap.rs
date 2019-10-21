use crate::delayqueue::DelayQueue;
use crate::delayqueue::Error as DelayQueueError;
use crate::delayqueue::Key as DelayQueueKey;
use crate::CloneSpawn;
use futures::ready;
use futures::task::{Context, Poll, SpawnExt};
use futures::{Future, FutureExt, Stream};
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::Send;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug)]
pub enum Error {
    DelayQueueError(DelayQueueError),
}

impl From<DelayQueueError> for Error {
    fn from(err: DelayQueueError) -> Error {
        Error::DelayQueueError(err)
    }
}

struct CacheMapInternal<K, V> {
    entries: HashMap<K, (V, DelayQueueKey)>,
    expirations: DelayQueue<K>,
}

impl<K, V> Unpin for CacheMapInternal<K, V> {}

impl<K, V> CacheMapInternal<K, V>
where
    K: Hash + Eq + Clone,
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Error>> {
        let unpinned = self.get_mut();
        while let Some(res) = ready!(std::pin::Pin::new(&mut unpinned.expirations).poll_next(cx)) {
            match res {
                Ok(entry) => {
                    unpinned.entries.remove(entry.get_ref());
                }
                Err(e) => {
                    eprintln!("DelayQueue error: {:?}", e);
                }
            }
        }
        Poll::Ready(Ok(()))
    }
}

pub struct CacheMap<K, V> {
    inner: Arc<Mutex<CacheMapInternal<K, V>>>,
}

struct CacheMapPollMe<K, V>(Arc<Mutex<CacheMapInternal<K, V>>>);

impl<K, V> Future for CacheMapPollMe<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut guard = self.0.lock().unwrap();
        let pinned = std::pin::Pin::new(guard.deref_mut());
        pinned.poll(cx)
    }
}

impl<K, V> CacheMap<K, V>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    K: 'static,
    V: 'static,
{
    pub fn new(handle: Box<dyn CloneSpawn>) -> Self {
        let inner = CacheMapInternal::new();
        let inner = Arc::new(Mutex::new(inner));
        let pollme = CacheMapPollMe(inner.clone());
        let pollme = pollme.map(|res| {
            if let Err(e) = res {
                eprintln!("CacheMap error: {:?}", e)
            }
        });
        let mut handle = handle;
        handle.spawn(pollme).unwrap();
        CacheMap { inner }
    }
}

impl<K, V> CacheMap<K, V>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
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
        self.inner.lock().unwrap().expirations.end();
    }
}
