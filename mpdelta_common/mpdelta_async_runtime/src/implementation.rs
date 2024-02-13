use crate::{AnyAsyncRuntime, JoinHandleWrapper};
use std::future::Future;
use tokio::runtime::Handle;
use tokio::task::{JoinError, JoinHandle};

impl AnyAsyncRuntime for Handle {
    type Err = JoinError;
    type JoinHandle<T: Send + 'static> = JoinHandle<T>;

    fn spawn<Fut>(&self, fut: Fut) -> JoinHandleWrapper<Self::JoinHandle<Fut::Output>>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send,
    {
        JoinHandleWrapper::new(self.spawn(fut))
    }
}

impl<T: Send> crate::JoinHandle for JoinHandle<T> {
    fn abort(&self) {
        JoinHandle::abort(self)
    }

    fn is_finished(&self) -> bool {
        JoinHandle::is_finished(self)
    }
}
