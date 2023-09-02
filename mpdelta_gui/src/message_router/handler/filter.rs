use crate::message_router::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::StaticCow;
use tokio::runtime::Handle;

pub struct Filter<F, T> {
    filter: F,
    tail: T,
}

pub struct FilterBuilder<P, F> {
    prev: P,
    filter: F,
}

impl<P, F> FilterBuilder<P, F> {
    pub(in crate::message_router::handler) fn new(prev: P, filter: F) -> Self {
        FilterBuilder { prev, filter }
    }
}

impl<Message, P, F> MessageHandlerBuilder<Message> for FilterBuilder<P, F>
where
    Message: Clone,
    P: MessageHandlerBuilder<Message>,
    F: Fn(&P::OutMessage) -> bool,
{
    type OutMessage = P::OutMessage;
}

impl<Message, Tail, P, F> IntoMessageHandler<Message, Tail> for FilterBuilder<P, F>
where
    Message: Clone,
    Tail: MessageHandler<P::OutMessage>,
    P: IntoMessageHandler<Message, Filter<F, Tail>>,
    F: Fn(&P::OutMessage) -> bool,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let FilterBuilder { prev, filter } = self;
        prev.into_message_handler(Filter { filter, tail })
    }
}

impl<Message, F, T> MessageHandler<Message> for Filter<F, T>
where
    Message: Clone,
    F: Fn(&Message) -> bool,
    T: MessageHandler<Message>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Handle) {
        let Filter { filter, tail } = self;
        if message.with_ref(filter) {
            tail.handle(message, runtime);
        }
    }
}
