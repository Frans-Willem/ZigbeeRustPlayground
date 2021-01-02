use std::task::{Context, Poll, Waker};

pub struct WakerStore {
    storage: Option<Waker>,
}

impl WakerStore {
    pub fn new() -> Self {
        Self { storage: None }
    }
    pub fn register(&mut self, waker: &Waker) {
        self.storage = Some(waker.clone());
    }
    pub fn wake(&mut self) {
        if let Some(waker) = self.storage.take() {
            waker.wake();
        }
    }
    pub fn pend<T>(&mut self, cx: &mut Context<'_>) -> Poll<T> {
        self.register(cx.waker());
        Poll::Pending
    }
    pub fn take(&mut self) -> Option<Waker> {
        self.storage.take()
    }
}
