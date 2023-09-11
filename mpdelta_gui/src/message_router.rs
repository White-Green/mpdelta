use crate::message_router::handler::empty::{EmptyHandler, EmptyHandlerBuilder};
use crate::message_router::handler::PairHandler;
use crate::message_router::static_cow::{Owned, StaticCow};
use mpdelta_async_runtime::AsyncRuntime;
use std::marker::PhantomData;

pub mod handler;
mod static_cow;

#[derive(Debug)]
pub struct MessageRouter<Handler, Runtime> {
    handler: Handler,
    runtime: Runtime,
}

impl MessageRouter<EmptyHandler, ()> {
    pub fn builder<Message, Runtime>() -> MessageRouterBuilder<Message, Runtime, EmptyHandler> {
        MessageRouterBuilder::new()
    }
}

impl<Handler, Runtime: AsyncRuntime<()> + Clone> MessageRouter<Handler, Runtime> {
    pub fn handle<Message>(&self, message: Message)
    where
        Message: Clone,
        Handler: MessageHandler<Message, Runtime>,
    {
        self.handler.handle(Owned(message), &self.runtime);
    }
}

#[derive(Debug)]
pub struct MessageRouterBuilder<Message, Runtime, Handler> {
    handler: Handler,
    _phantom: PhantomData<(Message, Runtime)>,
}

impl<Message, Runtime> MessageRouterBuilder<Message, Runtime, EmptyHandler> {
    pub fn new() -> MessageRouterBuilder<Message, Runtime, EmptyHandler> {
        MessageRouterBuilder { handler: EmptyHandler, _phantom: PhantomData }
    }
}

impl<Message, Runtime, Handler: MessageHandler<Message, Runtime>> MessageRouterBuilder<Message, Runtime, Handler> {
    pub fn handle<AdditionalHandler: MessageHandler<Message, Runtime>>(self, handler_builder: impl FnOnce(EmptyHandlerBuilder<Runtime>) -> AdditionalHandler) -> MessageRouterBuilder<Message, Runtime, PairHandler<AdditionalHandler, Handler>> {
        let MessageRouterBuilder { handler: current_handler, _phantom } = self;
        MessageRouterBuilder {
            handler: PairHandler(handler_builder(EmptyHandlerBuilder::new()), current_handler),
            _phantom,
        }
    }

    pub fn build(self, runtime: Runtime) -> MessageRouter<Handler, Runtime> {
        let MessageRouterBuilder { handler, .. } = self;
        MessageRouter { handler, runtime }
    }
}

pub trait MessageHandler<Message, Runtime> {
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime);
}

#[cfg(test)]
mod tests {
    use super::handler::MessageHandlerBuilder;
    use super::*;
    use crate::message_router::handler::{IntoAsyncFunctionHandler, IntoAsyncFunctionHandlerSingle, IntoFunctionHandler};
    use std::sync::atomic::AtomicUsize;
    use std::sync::{atomic, Arc};
    use std::time::Duration;
    use tokio::runtime::Handle;
    use tokio::sync::mpsc::error::TryRecvError;

    #[test]
    fn test_message_router() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let sum1 = Arc::new(AtomicUsize::new(0));
        let sum2 = Arc::new(AtomicUsize::new(0));
        let sum3 = Arc::new(AtomicUsize::new(0));
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.handle({
                    let sum1 = Arc::clone(&sum1);
                    move |message: usize| {
                        sum1.fetch_add(message, atomic::Ordering::SeqCst);
                    }
                })
            })
            .handle(|handler| {
                handler.handle({
                    let sum2 = Arc::clone(&sum2);
                    move |message: usize| {
                        sum2.fetch_add(message * 2, atomic::Ordering::SeqCst);
                    }
                })
            })
            .handle(|handler| {
                handler.handle({
                    let sum3 = Arc::clone(&sum3);
                    move |message: usize| {
                        sum3.fetch_add(message * 3, atomic::Ordering::SeqCst);
                    }
                })
            })
            .build(runtime.handle().clone());
        message_router.handle(1);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 1);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 2);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 3);
        message_router.handle(2);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 3);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 6);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 9);
        message_router.handle(3);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 6);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 12);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 18);
    }

    #[test]
    fn test_message_router_map() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let sum = AtomicUsize::new(0);
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.filter_map(|f| Some(f as i32)).map(|i| i as i8).filter_map(|i| Some(i as u64)).map(|i| i as usize).handle(|i| {
                    sum.fetch_add(i, atomic::Ordering::SeqCst);
                })
            })
            .build(runtime.handle().clone());
        message_router.handle(1.0);
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_message_router_complex_handler() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let sum1 = AtomicUsize::new(0);
        let sum2 = AtomicUsize::new(0);
        let sum3 = AtomicUsize::new(0);
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.handle(|message: usize| {
                    sum1.fetch_add(message, atomic::Ordering::SeqCst);
                })
            })
            .handle(|handler| {
                handler.filter(|&m| m >= 10).handle(|message: usize| {
                    sum2.fetch_add(message, atomic::Ordering::SeqCst);
                })
            })
            .handle(|handler| {
                handler.filter_map(|m| (m < 10).then_some(m)).handle(|message: usize| {
                    sum3.fetch_add(message, atomic::Ordering::SeqCst);
                })
            })
            .build(runtime.handle().clone());
        message_router.handle(1);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 1);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 0);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 1);
        message_router.handle(2);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 3);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 0);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 3);
        message_router.handle(3);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 6);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 0);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 6);
        message_router.handle(10);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 16);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 10);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 6);
        message_router.handle(11);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 27);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 21);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 6);
        message_router.handle(12);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 39);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 33);
        assert_eq!(sum3.load(atomic::Ordering::SeqCst), 6);
    }

    #[tokio::test]
    async fn test_message_router_complex_handler_async() {
        let sum1 = AtomicUsize::new(0);
        let sum2 = Arc::new(AtomicUsize::new(0));
        let (update_lock_sender, update_lock_receiver) = tokio::sync::mpsc::unbounded_channel();
        let update_lock_receiver = Arc::new(tokio::sync::Mutex::new(update_lock_receiver));
        let (update_event_sender, mut update_event_receiver) = tokio::sync::mpsc::unbounded_channel();
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.handle(|message: usize| {
                    sum1.fetch_add(message, atomic::Ordering::SeqCst);
                })
            })
            .handle(|handler| {
                handler.handle_async(|message: usize| {
                    let update_lock_receiver = Arc::clone(&update_lock_receiver);
                    let sum2 = Arc::clone(&sum2);
                    let update_event_sender = update_event_sender.clone();
                    async move {
                        update_lock_receiver.lock().await.recv().await.unwrap();
                        sum2.fetch_add(message, atomic::Ordering::SeqCst);
                        update_event_sender.send(()).unwrap();
                    }
                })
            })
            .build(Handle::current());
        message_router.handle(1);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 1);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 0);
        message_router.handle(2);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 3);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 0);
        message_router.handle(3);
        assert_eq!(sum1.load(atomic::Ordering::SeqCst), 6);
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 0);
        for _ in 0..3 {
            update_lock_sender.send(()).unwrap();
        }
        for _ in 0..3 {
            update_event_receiver.recv().await.unwrap();
        }
        assert_eq!(update_event_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum2.load(atomic::Ordering::SeqCst), 6);
    }

    #[tokio::test]
    async fn test_message_router_receiver() {
        let sum = Arc::new(AtomicUsize::new(0));
        let (update_lock_sender, mut update_lock_receiver) = tokio::sync::mpsc::unbounded_channel::<()>();
        let (end_process_sender, end_process_receiver) = tokio::sync::broadcast::channel::<()>(16);
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.handle_async_single(|mut message_receiver| {
                    let sum2 = Arc::clone(&sum);
                    let update_lock_sender = update_lock_sender.clone();
                    let mut end_process_receiver = end_process_receiver.resubscribe();
                    async move {
                        loop {
                            tokio::select! {
                                Some(message) = message_receiver.get_message() => {
                                    sum2.fetch_add(message, atomic::Ordering::SeqCst);
                                    let _ = update_lock_sender.send(());
                                }
                                _ = end_process_receiver.recv() => {
                                    break
                                }
                            }
                        }
                    }
                })
            })
            .build(Handle::current());
        message_router.handle(1);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 1);
        message_router.handle(2);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 3);
        message_router.handle(3);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 6);
        end_process_sender.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        message_router.handle(4);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 10);
        message_router.handle(5);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 15);
        message_router.handle(6);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 21);
        end_process_sender.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        message_router.handle(7);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 28);
        message_router.handle(8);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 36);
        message_router.handle(9);
        update_lock_receiver.recv().await;
        assert_eq!(update_lock_receiver.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(sum.load(atomic::Ordering::SeqCst), 45);
        end_process_sender.send(()).unwrap();
    }
}
