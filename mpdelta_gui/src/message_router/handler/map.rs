use crate::message_router::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::{Owned, StaticCow};
use mpdelta_async_runtime::AsyncRuntime;
use std::marker::PhantomData;

pub struct Map<F, T> {
    map: F,
    tail: T,
}

pub struct MapBuilder<P, F, Runtime> {
    prev: P,
    map: F,
    _phantom: PhantomData<Runtime>,
}

impl<P, F, Runtime> MapBuilder<P, F, Runtime> {
    pub(super) fn new(prev: P, map: F) -> Self {
        MapBuilder { prev, map, _phantom: PhantomData }
    }
}

impl<Message, Runtime, P, F, O> MessageHandlerBuilder<Message, Runtime> for MapBuilder<P, F, Runtime>
where
    Message: Clone,
    O: Clone,
    P: MessageHandlerBuilder<Message, Runtime>,
    F: Fn(P::OutMessage) -> O,
{
    type OutMessage = O;
}

impl<Message, Runtime, Tail, P, F, O> IntoMessageHandler<Message, Runtime, Tail> for MapBuilder<P, F, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Tail: MessageHandler<O, Runtime>,
    O: Clone,
    P: IntoMessageHandler<Message, Runtime, Map<F, Tail>>,
    F: Fn(P::OutMessage) -> O,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let MapBuilder { prev, map, _phantom } = self;
        prev.into_message_handler(Map { map, tail })
    }
}

impl<Message, Runtime, F, T, O> MessageHandler<Message, Runtime> for Map<F, T>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    O: Clone,
    F: Fn(Message) -> O,
    T: MessageHandler<O, Runtime>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
        let Map { map, tail } = self;
        let message = map(message.into_owned());
        tail.handle(Owned(message), runtime);
    }
}
