use futures::{task::Task, Async, Future, Poll, Stream};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::timer::delay_queue::Key as DelayQueueKey;
use tokio::timer::DelayQueue;

#[derive(Debug)]
pub enum Error {
    TimerError(tokio::timer::Error),
    CacheMapDropped,
}

impl From<tokio::timer::Error> for Error {
    fn from(timer_err: tokio::timer::Error) -> Error {
        Error::TimerError(timer_err)
    }
}

struct CacheMapInternal<K, V> {
    entries: HashMap<K, (V, DelayQueueKey)>,
    expirations: DelayQueue<K>,
    expirations_kicker: Option<Task>,
    dropped: bool,
}

impl<K, V> CacheMapInternal<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    fn new() -> CacheMapInternal<K, V> {
        CacheMapInternal {
            entries: HashMap::new(),
            expirations: DelayQueue::new(),
            expirations_kicker: None,
            dropped: false,
        }
    }

    fn insert(&mut self, key: K, value: V, ttl: Duration) -> Option<V> {
        let delay = self.expirations.insert(key.clone(), ttl);
        self.kick_expirations();
        if let Some((old_value, old_delay)) = self.entries.insert(key, (value, delay)) {
            self.expirations.remove(&old_delay);
            println!("Cachemap replaced, new size {}", self.entries.len());
            Some(old_value)
        } else {
            println!("Cachemap added, new size {}", self.entries.len());
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
        if self.dropped {
            Ok(Async::Ready(()))
        } else {
            while let Some(entry) = try_ready!(self.expirations.poll()) {
                self.entries.remove(entry.get_ref());
                println!("Cachemap dropped, new size {}", self.entries.len());
            }
            self.expirations_kicker = Some(futures::task::current());
            Ok(Async::NotReady)
        }
    }
}

impl<K, V> CacheMapInternal<K, V> {
    fn user_dropped(&mut self) {
        self.dropped = true;
        self.expirations.clear();
        self.kick_expirations()
    }
    fn kick_expirations(&mut self) {
        if let Some(task) = self.expirations_kicker.take() {
            task.notify();
        }
    }
}

pub struct CacheMap<K, V> {
    inner: Rc<Mutex<CacheMapInternal<K, V>>>,
}

struct CacheMapPollMe<K, V>(Rc<Mutex<CacheMapInternal<K, V>>>);

impl<K, V> Future for CacheMapPollMe<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let CacheMapPollMe(internal) = self;
        internal.lock().unwrap().poll()
    }
}

impl<K, V> CacheMap<K, V>
where
    K: Hash,
    K: std::cmp::Eq,
    K: std::clone::Clone,
    V: std::clone::Clone,
    K: 'static,
    V: 'static,
{
    pub fn new(handle: tokio_core::reactor::Handle) -> Self {
        let inner = CacheMapInternal::new();
        let inner = Rc::new(Mutex::new(inner));
        let pollme = CacheMapPollMe(inner.clone());
        let pollme = pollme.map_err(|e| eprintln!("CacheMap error: {:?}", e));
        handle.spawn(pollme);
        CacheMap { inner }
    }
}

impl<K, V> CacheMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
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
