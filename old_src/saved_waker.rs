use futures::task::{Context, Waker};

pub struct SavedWaker(Option<Waker>);

impl SavedWaker {
    pub fn new() -> Self {
        SavedWaker(None)
    }

    pub fn set(&mut self, cx: &mut Context) {
        self.0.replace(cx.waker().clone());
    }

    pub fn wake(&mut self) {
        if let Some(waker) = self.0.take() {
            waker.wake();
        }
    }
}
