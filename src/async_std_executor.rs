use async_std::task::JoinHandle;
use futures::future::Future;
use futures::future::FutureExt;
use futures::task::{Context, FutureObj, Poll, Spawn, SpawnError};
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AsyncStdExecutor {
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl AsyncStdExecutor {
    pub fn new() -> AsyncStdExecutor {
        AsyncStdExecutor {
            handles: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Spawn for AsyncStdExecutor {
    fn spawn_obj(&self, future: FutureObj<'static, ()>) -> Result<(), SpawnError> {
        let join_handle = async_std::task::spawn(future);
        self.handles.lock().unwrap().push(join_handle);
        Ok(())
    }
}

impl Future for AsyncStdExecutor {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        let joinhandles = std::mem::take(self.handles.lock().unwrap().deref_mut());
        for mut handle in joinhandles.into_iter() {
            if let Poll::Pending = handle.poll_unpin(cx) {
                self.handles.lock().unwrap().push(handle);
            }
        }
        let num_remaining = self.handles.lock().unwrap().len();
        if num_remaining == 0 {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
