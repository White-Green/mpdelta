use crate::message_router::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::{Owned, StaticCow};
use futures::FutureExt;
use mpdelta_async_runtime::AsyncRuntime;
use std::future::IntoFuture;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct Then<F, T> {
    map: F,
    tail: Arc<T>,
}

pub struct ThenBuilder<P, F, Runtime> {
    prev: P,
    map: F,
    _phantom: PhantomData<Runtime>,
}

impl<P, F, Runtime> ThenBuilder<P, F, Runtime> {
    pub(super) fn new(prev: P, map: F) -> Self {
        ThenBuilder { prev, map, _phantom: PhantomData }
    }
}

impl<Message, Runtime, P, F, Fut> MessageHandlerBuilder<Message, Runtime> for ThenBuilder<P, F, Runtime>
where
    Message: Clone,
    Fut: IntoFuture,
    Fut::IntoFuture: 'static,
    Fut::Output: Clone,
    P: MessageHandlerBuilder<Message, Runtime>,
    F: Fn(P::OutMessage) -> Fut,
{
    type OutMessage = Fut::Output;
}

impl<Message, Runtime, Tail, P, F, Fut> IntoMessageHandler<Message, Runtime, Tail> for ThenBuilder<P, F, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Fut: IntoFuture,
    Fut::IntoFuture: 'static,
    Fut::Output: Clone,
    P: IntoMessageHandler<Message, Runtime, Then<F, Tail>>,
    F: Fn(P::OutMessage) -> Fut,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let ThenBuilder { prev, map, _phantom } = self;
        prev.into_message_handler(Then { map, tail: Arc::new(tail) })
    }
}

impl<Message, Runtime, F, T, Fut> MessageHandler<Message, Runtime> for Then<F, T>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone + 'static,
    Fut: IntoFuture,
    Fut::IntoFuture: Send + 'static,
    Fut::Output: Clone,
    F: Fn(Message) -> Fut,
    T: MessageHandler<Fut::Output, Runtime> + Send + Sync + 'static,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
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
