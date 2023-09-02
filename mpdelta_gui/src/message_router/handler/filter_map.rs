use crate::message_router::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::{Owned, StaticCow};
use tokio::runtime::Handle;

pub struct FilterMap<F, T> {
    filter: F,
    tail: T,
}

pub struct FilterMapBuilder<P, F> {
    prev: P,
    filter: F,
}

impl<P, F> FilterMapBuilder<P, F> {
    pub(super) fn new(prev: P, filter: F) -> Self {
        FilterMapBuilder { prev, filter }
    }
}

impl<Message, P, F, Out> MessageHandlerBuilder<Message> for FilterMapBuilder<P, F>
where
    Message: Clone,
    Out: Clone,
    P: MessageHandlerBuilder<Message>,
    F: Fn(P::OutMessage) -> Option<Out>,
{
    type OutMessage = Out;
}

impl<Message, Tail, P, F, Out> IntoMessageHandler<Message, Tail> for FilterMapBuilder<P, F>
where
    Message: Clone,
    Tail: MessageHandler<Out>,
    Out: Clone,
    P: IntoMessageHandler<Message, FilterMap<F, Tail>>,
    F: Fn(P::OutMessage) -> Option<Out>,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let FilterMapBuilder { prev, filter } = self;
        prev.into_message_handler(FilterMap { filter, tail })
    }
}

impl<Message, F, T, O> MessageHandler<Message> for FilterMap<F, T>
where
    Message: Clone,
    O: Clone,
    F: Fn(Message) -> Option<O>,
    T: MessageHandler<O>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Handle) {
        let FilterMap { filter, tail } = self;
        let Some(message) = filter(message.into_owned()) else {
            return;
        };
        tail.handle(Owned(message), runtime);
    }
}
