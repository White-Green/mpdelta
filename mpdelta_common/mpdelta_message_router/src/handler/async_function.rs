use crate::static_cow::StaticCow;
use crate::MessageHandler;
use mpdelta_async_runtime::{AsyncRuntime, JoinHandleWrapper};
use std::future::IntoFuture;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Mutex, PoisonError};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub struct AsyncFunctionHandler<F> {
    f: F,
}

impl<F> AsyncFunctionHandler<F> {
    pub(super) fn new(f: F) -> AsyncFunctionHandler<F> {
        AsyncFunctionHandler { f }
    }
}

impl<Message, Runtime, F, Future> MessageHandler<Message, Runtime> for AsyncFunctionHandler<F>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
    F: Fn(Message) -> Future,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
        let message = message.into_owned();
        let future = (self.f)(message);
        runtime.spawn(future.into_future());
    }
}

#[derive(Debug)]
pub struct MessageReceiver<T> {
    receiver: Option<UnboundedReceiver<T>>,
    receiver_return: SyncSender<UnboundedReceiver<T>>,
}

impl<T> MessageReceiver<T> {
    fn new(receiver: UnboundedReceiver<T>, receiver_return: SyncSender<UnboundedReceiver<T>>) -> MessageReceiver<T> {
        MessageReceiver { receiver: Some(receiver), receiver_return }
    }
}

impl<T> Drop for MessageReceiver<T> {
    fn drop(&mut self) {
        let _ = self.receiver_return.send(self.receiver.take().unwrap());
    }
}

impl<T> MessageReceiver<T> {
    pub async fn get_message(&mut self) -> Option<T> {
        self.receiver.as_mut()?.recv().await
    }
}

type RunningTask<JoinHandle> = Option<JoinHandleWrapper<JoinHandle>>;
pub struct AsyncFunctionHandlerSingle<Message, F, JoinHandle> {
    f: F,
    handle: Mutex<(RunningTask<JoinHandle>, Receiver<UnboundedReceiver<Message>>)>,
    receiver_return: SyncSender<UnboundedReceiver<Message>>,
    message_sender: UnboundedSender<Message>,
}

impl<Message: Clone, F> AsyncFunctionHandlerSingle<Message, F, ()> {
    pub(super) fn new<Runtime: AsyncRuntime<()>>(f: F) -> AsyncFunctionHandlerSingle<Message, F, Runtime::JoinHandle> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let (sender_return, receiver_return) = std::sync::mpsc::sync_channel(1);
        sender_return.send(receiver).unwrap();
        AsyncFunctionHandlerSingle {
            f,
            handle: Mutex::new((None, receiver_return)),
            receiver_return: sender_return,
            message_sender: sender,
        }
    }
}

impl<Message, Runtime, F, Future> MessageHandler<Message, Runtime> for AsyncFunctionHandlerSingle<Message, F, Runtime::JoinHandle>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
    F: Fn(MessageReceiver<Message>) -> Future,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
        let message = message.into_owned();
        self.message_sender.send(message.clone()).ok().unwrap();
        let mut lock = self.handle.lock().unwrap_or_else(PoisonError::into_inner);
        match &mut *lock {
            (Some(running_task), _) if !running_task.is_finished() => drop(lock),
            (running_task, message_receiver_return) => {
                if !running_task.as_ref().is_some_and(|handle| !handle.is_finished()) {
                    let future = (self.f)(MessageReceiver::new(message_receiver_return.recv().unwrap(), self.receiver_return.clone()));
                    *running_task = Some(runtime.spawn(future.into_future()));
                }
                drop(lock);
            }
        }
    }
}
