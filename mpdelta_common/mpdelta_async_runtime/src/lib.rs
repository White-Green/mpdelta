use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

#[cfg(feature = "tokio")]
pub mod implementation;

pub trait JoinHandle: Future + Send + Sync {
    fn abort(&self);
    fn is_finished(&self) -> bool;
}

impl<T: JoinHandle + Unpin + ?Sized> JoinHandle for Box<T> {
    fn abort(&self) {
        T::abort(self)
    }

    fn is_finished(&self) -> bool {
        T::is_finished(self)
    }
}

impl<T: JoinHandle + ?Sized> JoinHandle for Pin<Box<T>> {
    fn abort(&self) {
        T::abort(self)
    }

    fn is_finished(&self) -> bool {
        T::is_finished(self)
    }
}

pub trait AnyAsyncRuntime: Send + Sync {
    type Err: Error + Send + 'static;
    type JoinHandle<T: Send + 'static>: JoinHandle<Output = Result<T, Self::Err>> + 'static;
    fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<Fut::Output>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send;
}

pub trait AsyncRuntime<Out>: Send + Sync {
    type Err: Error + Send + 'static;
    type JoinHandle: JoinHandle<Output = Result<Out, Self::Err>> + 'static;
    fn spawn<Fut: Future<Output = Out> + Send + 'static>(&self, fut: Fut) -> Self::JoinHandle;
}

pub trait AsyncRuntimeDyn<Out>: Send + Sync {
    fn spawn_dyn(&self, fut: Pin<Box<dyn Future<Output = Out> + Send + 'static>>) -> Pin<Box<dyn JoinHandle<Output = Result<Out, BoxedError>>>>;
}

impl<Runtime: AnyAsyncRuntime, T: Send + 'static> AsyncRuntime<T> for Runtime {
    type Err = Runtime::Err;
    type JoinHandle = Runtime::JoinHandle<T>;

    fn spawn<Fut: Future<Output = T> + Send + 'static>(&self, fut: Fut) -> Self::JoinHandle {
        AnyAsyncRuntime::spawn(self, fut)
    }
}

impl<Out: 'static, T> AsyncRuntimeDyn<Out> for T
where
    T: AsyncRuntime<Out>,
{
    fn spawn_dyn(&self, fut: Pin<Box<dyn Future<Output = Out> + Send + 'static>>) -> Pin<Box<dyn JoinHandle<Output = Result<Out, BoxedError>>>> {
        Box::pin(BoxedErrorFuture(self.spawn(fut)))
    }
}

impl<Out: 'static> AsyncRuntime<Out> for dyn AsyncRuntimeDyn<Out> {
    type Err = BoxedError;
    type JoinHandle = Pin<Box<dyn JoinHandle<Output = Result<Out, Self::Err>>>>;

    fn spawn<Fut: Future<Output = Out> + Send + 'static>(&self, fut: Fut) -> Self::JoinHandle {
        self.spawn_dyn(Box::pin(fut))
    }
}

impl<Out, O> AsyncRuntime<Out> for Arc<O>
where
    O: AsyncRuntime<Out> + ?Sized,
{
    type Err = O::Err;
    type JoinHandle = O::JoinHandle;

    fn spawn<Fut: Future<Output = Out> + Send + 'static>(&self, fut: Fut) -> Self::JoinHandle {
        O::spawn(self, fut)
    }
}

pub struct BoxedError(pub Box<dyn Error + Send + 'static>);

impl Debug for BoxedError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for BoxedError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Error for BoxedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Error::source(&*self.0)
    }
}

pub struct BoxedErrorFuture<T>(T);

impl<T, O, E> Future for BoxedErrorFuture<T>
where
    T: Future<Output = Result<O, E>>,
    E: Error + Send + 'static,
{
    type Output = Result<O, BoxedError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: fieldの値をclosure外部とやり取りしていないので安全
        // ref: https://doc.rust-lang.org/std/pin/index.html#pinning-is-structural-for-field
        let future = unsafe { self.map_unchecked_mut(|BoxedErrorFuture(f)| f) };
        match future.poll(cx) {
            Poll::Ready(Ok(value)) => Poll::Ready(Ok(value)),
            Poll::Ready(Err(err)) => Poll::Ready(Err(BoxedError(Box::new(err)))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T, O> JoinHandle for BoxedErrorFuture<T>
where
    T: JoinHandle,
    BoxedErrorFuture<T>: Future<Output = Result<O, BoxedError>>,
{
    fn abort(&self) {
        self.0.abort()
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn assert_runtime<T: AsyncRuntime<()>>() {}

    fn assert_runtime_clone<T: AsyncRuntime<()> + Clone>() {}

    #[test]
    fn test() {
        assert_runtime::<Arc<dyn AsyncRuntimeDyn<()>>>();
        assert_runtime_clone::<Arc<dyn AsyncRuntimeDyn<()>>>();
    }
}
