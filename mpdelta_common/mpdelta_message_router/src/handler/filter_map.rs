use crate::handler::{IntoMessageHandler, MessageHandler, MessageHandlerBuilder};
use crate::static_cow::{Owned, StaticCow};
use mpdelta_async_runtime::AsyncRuntime;
use std::marker::PhantomData;

pub struct FilterMap<F, T> {
    filter: F,
    tail: T,
}

pub struct FilterMapBuilder<P, F, Runtime> {
    prev: P,
    filter: F,
    _phantom: PhantomData<Runtime>,
}

impl<P, F, Runtime> FilterMapBuilder<P, F, Runtime> {
    pub(super) fn new(prev: P, filter: F) -> Self {
        FilterMapBuilder { prev, filter, _phantom: PhantomData }
    }
}

impl<Message, Runtime, P, F, Out> MessageHandlerBuilder<Message, Runtime> for FilterMapBuilder<P, F, Runtime>
where
    Message: Clone,
    Out: Clone,
    P: MessageHandlerBuilder<Message, Runtime>,
    F: Fn(P::OutMessage) -> Option<Out>,
{
    type OutMessage = Out;
}

impl<Message, Runtime, Tail, P, F, Out> IntoMessageHandler<Message, Runtime, Tail> for FilterMapBuilder<P, F, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Tail: MessageHandler<Out, Runtime>,
    Out: Clone,
    P: IntoMessageHandler<Message, Runtime, FilterMap<F, Tail>>,
    F: Fn(P::OutMessage) -> Option<Out>,
{
    type Out = P::Out;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        let FilterMapBuilder { prev, filter, _phantom } = self;
        prev.into_message_handler(FilterMap { filter, tail })
    }
}

impl<Message, Runtime, F, T, O> MessageHandler<Message, Runtime> for FilterMap<F, T>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    O: Clone,
    F: Fn(Message) -> Option<O>,
    T: MessageHandler<O, Runtime>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
        let FilterMap { filter, tail } = self;
        let Some(message) = filter(message.into_owned()) else {
            return;
        };
        tail.handle(Owned(message), runtime);
    }
}
