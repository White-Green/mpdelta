use crate::message_router::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::{Owned, StaticCow};
use tokio::runtime::Handle;

pub struct Map<F, T> {
    map: F,
    tail: T,
}

pub struct MapBuilder<P, F> {
    prev: P,
    map: F,
}

impl<P, F> MapBuilder<P, F> {
    pub(super) fn new(prev: P, map: F) -> Self {
        MapBuilder { prev, map }
    }
}

impl<Message, P, F, O> MessageHandlerBuilder<Message> for MapBuilder<P, F>
where
    Message: Clone,
    O: Clone,
    P: MessageHandlerBuilder<Message>,
    F: Fn(P::OutMessage) -> O,
{
    type OutMessage = O;
}

impl<Message, Tail, P, F, O> IntoMessageHandler<Message, Tail> for MapBuilder<P, F>
where
    Message: Clone,
    Tail: MessageHandler<O>,
    O: Clone,
    P: IntoMessageHandler<Message, Map<F, Tail>>,
    F: Fn(P::OutMessage) -> O,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let MapBuilder { prev, map } = self;
        prev.into_message_handler(Map { map, tail })
    }
}

impl<Message, F, T, O> MessageHandler<Message> for Map<F, T>
where
    Message: Clone,
    O: Clone,
    F: Fn(Message) -> O,
    T: MessageHandler<O>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Handle) {
        let Map { map, tail } = self;
        let message = map(message.into_owned());
        tail.handle(Owned(message), runtime);
    }
}
