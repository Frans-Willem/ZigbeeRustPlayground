use futures::{Future, TryFuture, TryFutureExt};

pub type RetTryFuture<V, E> = Box<Future<Output = Result<V, E>> + Send + Unpin>;
pub type RetFuture<V> = Box<Future<Output = V> + Send + Unpin>;

pub fn return_future<F: Future + Send + 'static + Unpin>(fut: F) -> RetFuture<F::Output> {
    Box::new(fut)
}

pub fn return_try_future<F: TryFuture + Send + 'static + Unpin>(
    fut: F,
) -> RetTryFuture<F::Ok, F::Error> {
    Box::new(fut.into_future())
}
