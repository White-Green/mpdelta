use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use futures::FutureExt;
use once_cell::sync::OnceCell;
use std::future::Future;
use std::hash::Hash;
use std::marker::PhantomData;
use std::mem;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

pub struct CellByNeed<T> {
    shared: Arc<Inner<T>>,
}

pub struct CellSetter<T> {
    shared: Arc<Inner<T>>,
}

struct Inner<T> {
    cell: OnceCell<T>,
    waiters: Mutex<Vec<Waker>>,
}

impl<T> CellSetter<T> {
    pub fn set(self, value: T) {
        let mut lock = self.shared.waiters.lock().unwrap();
        assert!(self.shared.cell.set(value).is_ok());
        mem::take(&mut *lock).into_iter().for_each(Waker::wake);
        drop(lock);
    }
}

impl<T> Clone for CellByNeed<T> {
    fn clone(&self) -> Self {
        CellByNeed { shared: Arc::clone(&self.shared) }
    }
}

impl<T: Clone> Future for CellByNeed<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut lock = self.shared.waiters.lock().unwrap();
        let ret = match self.shared.cell.get().cloned() {
            Some(value) => Poll::Ready(value),
            None => {
                lock.push(cx.waker().clone());
                Poll::Pending
            }
        };
        drop(lock);
        ret
    }
}

pub fn cell_by_need<T: Clone>() -> (CellByNeed<T>, CellSetter<T>) {
    let shared = Arc::new(Inner { cell: OnceCell::new(), waiters: Mutex::new(Vec::new()) });
    (CellByNeed { shared: Arc::clone(&shared) }, CellSetter { shared })
}

struct FunctionByNeedInner<Arg, Ret, F, Fut> {
    cells: DashMap<Arg, CellByNeed<Ret>>,
    function: F,
    phantom: PhantomData<Fut>,
}

// SAFETY:
// 関係あるのはPhantomData<Fut>のみなので安全
unsafe impl<Arg: Sync, Ret: Sync, F: Sync, Fut> Sync for FunctionByNeedInner<Arg, Ret, F, Fut> {}

pub struct FunctionByNeed<Arg, Ret, F = DynFn<Arg, DynFuture<'static, Ret>>, Fut = DynFuture<'static, Ret>> {
    inner: Arc<FunctionByNeedInner<Arg, Ret, F, Fut>>,
}

impl<Arg, Ret, F, Fut> Clone for FunctionByNeed<Arg, Ret, F, Fut> {
    fn clone(&self) -> Self {
        FunctionByNeed { inner: Arc::clone(&self.inner) }
    }
}

pub trait FnWrapper<Arg, Ret> {
    fn call(&self, arg: Arg) -> Ret;
}

impl<Arg, Ret, F: Fn(Arg) -> Ret> FnWrapper<Arg, Ret> for F {
    fn call(&self, arg: Arg) -> Ret {
        self(arg)
    }
}

pub struct DynFn<Arg, Ret>(pub Box<dyn Fn(Arg) -> Ret + Send + Sync>);

impl<Arg, Ret> FnWrapper<Arg, Ret> for DynFn<Arg, Ret> {
    fn call(&self, arg: Arg) -> Ret {
        self.0(arg)
    }
}

pub struct DynFuture<'a, Ret>(pub Pin<Box<dyn Future<Output = Ret> + Send + 'a>>);

impl<'a, Ret> Future for DynFuture<'a, Ret> {
    type Output = Ret;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

impl<Arg: Clone + Eq + Hash + Send + Sync + 'static, Ret: Clone + Send + Sync + 'static> FunctionByNeed<Arg, Ret> {
    pub fn new_dyn<Fut: Future<Output = Ret> + Send + Sync + 'static>(function: impl Fn(Arg) -> Fut + Send + Sync + 'static) -> FunctionByNeed<Arg, Ret> {
        FunctionByNeed {
            inner: Arc::new(FunctionByNeedInner {
                cells: DashMap::new(),
                function: DynFn(Box::new(move |arg| DynFuture(function(arg).boxed()))),
                phantom: Default::default(),
            }),
        }
    }
}

impl<Arg: Clone + Eq + Hash + Send + Sync + 'static, Ret: Clone + Send + Sync + 'static, F: FnWrapper<Arg, Fut> + Send + Sync + 'static, Fut: Future<Output = Ret> + Send + 'static> FunctionByNeed<Arg, Ret, F, Fut> {
    pub fn new(function: F) -> FunctionByNeed<Arg, Ret, F, Fut> {
        FunctionByNeed {
            inner: Arc::new(FunctionByNeedInner { cells: DashMap::new(), function, phantom: Default::default() }),
        }
    }

    pub fn call(&self, arg: Arg) -> Pin<Box<dyn Future<Output = Ret> + Send + 'static>> {
        match self.inner.cells.entry(arg) {
            Entry::Occupied(entry) => Box::pin(CellByNeed::clone(entry.get())),
            Entry::Vacant(entry) => {
                let arg = entry.key().clone();
                let (cell_by_need, setter) = cell_by_need();
                entry.insert(cell_by_need);
                let inner = Arc::clone(&self.inner);
                Box::pin(self.inner.function.call(arg.clone()).map(move |ret| {
                    setter.set(ret.clone());
                    inner.cells.remove(&arg);
                    ret
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter;
    use std::sync::atomic;
    use std::sync::atomic::AtomicUsize;
    use tokio::time::Duration;
    use tokio::time::Instant;

    #[tokio::test]
    async fn test_cell_by_need() {
        for _ in 0..1_000 {
            let (cell, setter) = cell_by_need::<u32>();
            let until = Instant::now() + Duration::from_micros(100);
            let mut threads = Vec::with_capacity(31);
            let mut cells = iter::repeat(cell);
            for cell in cells.by_ref().take(10) {
                threads.push(tokio::spawn(async move {
                    tokio::time::sleep_until(until).await;
                    assert_eq!(cell.await, 42);
                }));
            }
            threads.push(tokio::spawn(async move {
                tokio::time::sleep_until(until).await;
                setter.set(42);
            }));
            for cell in cells.by_ref().take(10) {
                threads.push(tokio::spawn(async move {
                    tokio::time::sleep_until(until).await;
                    assert_eq!(cell.await, 42);
                }));
            }
            tokio::time::sleep_until(until).await;
            for cell in cells.take(10) {
                threads.push(tokio::spawn(async move {
                    assert_eq!(cell.await, 42);
                }));
            }
            for thread in threads {
                thread.await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn test_function_by_need() {
        let counter = Arc::new(AtomicUsize::new(0));
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let guard = lock.lock().await;
        let function = FunctionByNeed::new({
            let lock = Arc::clone(&lock);
            let counter = Arc::clone(&counter);
            move |arg| {
                counter.fetch_add(1, atomic::Ordering::SeqCst);
                let lock = Arc::clone(&lock);
                async move {
                    lock.lock().await;
                    arg
                }
            }
        });
        assert_eq!(counter.load(atomic::Ordering::SeqCst), 0);
        let threads = (0..10).map(|_| tokio::spawn(function.call(0))).collect::<Vec<_>>();
        assert_eq!(function.inner.cells.len(), 1);
        drop(guard);
        for thread in threads {
            assert_eq!(thread.await.unwrap(), 0);
        }
        assert_eq!(counter.load(atomic::Ordering::SeqCst), 1);
        assert_eq!(function.inner.cells.len(), 0);
        let guard = lock.lock().await;
        let threads = (0..10).map(|i| tokio::spawn(function.call(i))).collect::<Vec<_>>();
        assert_eq!(function.inner.cells.len(), 10);
        drop(guard);
        for (i, thread) in threads.into_iter().enumerate() {
            assert_eq!(thread.await.unwrap(), i);
        }
        assert_eq!(counter.load(atomic::Ordering::SeqCst), 11);
        assert_eq!(function.inner.cells.len(), 0);
    }
}
