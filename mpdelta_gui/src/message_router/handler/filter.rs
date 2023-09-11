use crate::message_router::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::StaticCow;
use mpdelta_async_runtime::AsyncRuntime;
use std::marker::PhantomData;

pub struct Filter<F, T> {
    filter: F,
    tail: T,
}

pub struct FilterBuilder<P, F, Runtime> {
    prev: P,
    filter: F,
    _phantom: PhantomData<Runtime>,
}

impl<P, F, Runtime> FilterBuilder<P, F, Runtime> {
    pub(in crate::message_router::handler) fn new(prev: P, filter: F) -> Self {
        FilterBuilder { prev, filter, _phantom: PhantomData }
    }
}

impl<Message, P, F, Runtime> MessageHandlerBuilder<Message, Runtime> for FilterBuilder<P, F, Runtime>
where
    Message: Clone,
    P: MessageHandlerBuilder<Message, Runtime>,
    F: Fn(&P::OutMessage) -> bool,
    Runtime: AsyncRuntime<()> + Clone,
{
    type OutMessage = P::OutMessage;
}

impl<Message, Runtime, Tail, P, F> IntoMessageHandler<Message, Runtime, Tail> for FilterBuilder<P, F, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Tail: MessageHandler<P::OutMessage, Runtime>,
    P: IntoMessageHandler<Message, Runtime, Filter<F, Tail>>,
    F: Fn(&P::OutMessage) -> bool,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let FilterBuilder { prev, filter, _phantom } = self;
        prev.into_message_handler(Filter { filter, tail })
    }
}

impl<Message, Runtime, F, T> MessageHandler<Message, Runtime> for Filter<F, T>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    F: Fn(&Message) -> bool,
    T: MessageHandler<Message, Runtime>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
        let Filter { filter, tail } = self;
        if message.with_ref(filter) {
            tail.handle(message, runtime);
        }
    }
}
