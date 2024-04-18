use arc_swap::{ArcSwapOption, Guard};
use futures::FutureExt;
use std::future::IntoFuture;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

pub struct LazyInit<T> {
    slot: Arc<ArcSwapOption<T>>,
    init: JoinHandle<()>,
}

impl<T> LazyInit<T>
where
    T: Send + Sync + 'static,
{
    pub fn new<F>(fut: F, runtime: &Handle) -> LazyInit<T>
    where
        F: IntoFuture<Output = T>,
        F::IntoFuture: Send + 'static,
    {
        let slot = Arc::new(ArcSwapOption::default());
        let fut = fut.into_future().map({
            let slot = Arc::clone(&slot);
            move |result| {
                slot.store(Some(Arc::new(result)));
            }
        });
        LazyInit { slot, init: runtime.spawn(fut) }
    }

    pub fn get(&self) -> Guard<Option<Arc<T>>> {
        self.slot.load()
    }
}

impl<T> Drop for LazyInit<T> {
    fn drop(&mut self) {
        self.init.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lazy_init() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let lazy = LazyInit::new(
            async move {
                receiver.await.unwrap();
                42
            },
            &Handle::current(),
        );
        assert!(lazy.get().is_none());
        sender.send(()).unwrap();
        loop {
            let guard = lazy.get();
            match guard.as_deref() {
                None => tokio::task::yield_now().await,
                Some(&v) => {
                    assert_eq!(v, 42);
                    return;
                }
            }
        }
    }
}
