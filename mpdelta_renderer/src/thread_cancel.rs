use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::{atomic, Arc};
use std::task::{Context, Poll};
use tokio::task::JoinHandle;

pub(crate) struct AutoCancelJoinHandle<T>(JoinHandle<T>);

impl<T> Future for AutoCancelJoinHandle<T> {
    type Output = <JoinHandle<T> as Future>::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> Deref for AutoCancelJoinHandle<T> {
    type Target = JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for AutoCancelJoinHandle<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Drop for AutoCancelJoinHandle<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) trait AutoCancellable {
    type Output;
    fn auto_cancel(self) -> AutoCancelJoinHandle<Self::Output>;
}

impl<T> AutoCancellable for JoinHandle<T> {
    type Output = T;

    fn auto_cancel(self) -> AutoCancelJoinHandle<Self::Output> {
        AutoCancelJoinHandle(self)
    }
}

pub(crate) struct CancellationGuard {
    cancelled: Arc<AtomicBool>,
}

impl CancellationGuard {
    pub(crate) fn new() -> CancellationGuard {
        CancellationGuard { cancelled: Arc::new(AtomicBool::new(false)) }
    }

    pub(crate) fn token(&self) -> CancellationToken {
        CancellationToken { cancelled: Arc::clone(&self.cancelled) }
    }

    pub(crate) fn cancel(self) {}
}

impl Drop for CancellationGuard {
    fn drop(&mut self) {
        self.cancelled.store(true, atomic::Ordering::Release);
    }
}

#[derive(Clone)]
pub(crate) struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub(crate) fn is_canceled(&self) -> bool {
        self.cancelled.load(atomic::Ordering::Acquire)
    }

    pub(crate) fn assert_not_canceled(&self) {
        assert!(!self.is_canceled(), "canceled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{atomic, Arc};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_auto_cancel_join_handle() {
        let handle = tokio::spawn(async { 42 });
        let handle = handle.auto_cancel();
        assert_matches!(handle.await, Ok(42));

        let counter = Arc::new(AtomicUsize::new(0));
        let (sender1, receiver1) = oneshot::channel();
        let (sender2, receiver2) = oneshot::channel();
        let (sender3, receiver3) = oneshot::channel();
        let handle = tokio::spawn({
            let counter = Arc::clone(&counter);
            async move {
                let handle = tokio::spawn(async move {
                    receiver1.await.unwrap();
                    counter.fetch_add(1, atomic::Ordering::SeqCst);
                    sender2.send(()).unwrap();
                });
                tokio::select! {
                    _ = handle => { unreachable!() }
                    _ = receiver3 => {}
                }
            }
        });
        sender3.send(()).unwrap();
        handle.await.unwrap();
        sender1.send(()).unwrap();
        receiver2.await.unwrap();
        assert_eq!(counter.load(atomic::Ordering::SeqCst), 1);

        let counter = Arc::new(AtomicUsize::new(0));
        let (sender1, receiver1) = oneshot::channel();
        let (sender2, receiver2) = oneshot::channel();
        let (sender3, receiver3) = oneshot::channel();
        let handle = tokio::spawn({
            let counter = Arc::clone(&counter);
            async move {
                let handle = tokio::spawn(async move {
                    receiver1.await.unwrap();
                    counter.fetch_add(1, atomic::Ordering::SeqCst);
                    sender2.send(()).unwrap();
                })
                .auto_cancel();
                tokio::select! {
                    _ = handle => { unreachable!() }
                    _ = receiver3 => {}
                }
            }
        });
        sender3.send(()).unwrap();
        handle.await.unwrap();
        sender1.send(()).unwrap_err();
        receiver2.await.unwrap_err();
        assert_eq!(counter.load(atomic::Ordering::SeqCst), 0);

        let counter = Arc::new(AtomicUsize::new(0));
        let (sender1, receiver1) = oneshot::channel();
        let (sender2, receiver2) = oneshot::channel();
        let (sender3, receiver3) = oneshot::channel();
        let handle = tokio::spawn({
            let counter = Arc::clone(&counter);
            async move {
                let handle = tokio::spawn(async move {
                    receiver1.await.unwrap();
                    counter.fetch_add(1, atomic::Ordering::SeqCst);
                    sender2.send(()).unwrap();
                })
                .auto_cancel();
                tokio::select! {
                    _ = handle => {}
                    _ = receiver3 => { unreachable!() }
                }
            }
        });
        sender1.send(()).unwrap();
        receiver2.await.unwrap();
        assert_eq!(counter.load(atomic::Ordering::SeqCst), 1);
        handle.await.unwrap();
        sender3.send(()).unwrap_err();
    }
}
