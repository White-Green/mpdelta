use crate::message_router::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::{Owned, StaticCow};
use futures::FutureExt;
use std::future::IntoFuture;
use std::sync::Arc;
use tokio::runtime::Handle;

pub struct Then<F, T> {
    map: F,
    tail: Arc<T>,
}

pub struct ThenBuilder<P, F> {
    prev: P,
    map: F,
}

impl<P, F> ThenBuilder<P, F> {
    pub(super) fn new(prev: P, map: F) -> Self {
        ThenBuilder { prev, map }
    }
}

impl<Message, P, F, Fut> MessageHandlerBuilder<Message> for ThenBuilder<P, F>
where
    Message: Clone,
    Fut: IntoFuture,
    Fut::IntoFuture: 'static,
    Fut::Output: Clone,
    P: MessageHandlerBuilder<Message>,
    F: Fn(P::OutMessage) -> Fut,
{
    type OutMessage = Fut::Output;
}

impl<Message, Tail, P, F, Fut> IntoMessageHandler<Message, Tail> for ThenBuilder<P, F>
where
    Message: Clone,
    Fut: IntoFuture,
    Fut::IntoFuture: 'static,
    Fut::Output: Clone,
    P: IntoMessageHandler<Message, Then<F, Tail>>,
    F: Fn(P::OutMessage) -> Fut,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let ThenBuilder { prev, map } = self;
        prev.into_message_handler(Then { map, tail: Arc::new(tail) })
    }
}

impl<Message, F, T, Fut> MessageHandler<Message> for Then<F, T>
where
    Message: Clone,
    Fut: IntoFuture,
    Fut::IntoFuture: Send + 'static,
    Fut::Output: Clone,
    F: Fn(Message) -> Fut,
    T: MessageHandler<Fut::Output> + Send + Sync + 'static,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Handle) {
        let Then { map, tail } = self;
        let message = map(message.into_owned());
        let future = {
            let tail = Arc::clone(tail);
            let runtime = runtime.clone();
            message.into_future().map(move |message| {
                tail.handle(Owned(message), &runtime);
            })
        };
        runtime.spawn(future);
    }
}
