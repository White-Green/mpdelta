use crate::message_router::handler::empty::EmptyHandler;
use crate::message_router::handler::{IntoMessageHandler, MessageHandler, PairHandler};
use std::marker::PhantomData;

pub struct MultipleBuilder<Message, P, T> {
    prev: P,
    tail: T,
    _phantom: PhantomData<Message>,
}

impl<Message, P> MultipleBuilder<Message, P, EmptyHandler> {
    pub(super) fn new(prev: P) -> MultipleBuilder<Message, P, EmptyHandler> {
        MultipleBuilder { prev, tail: EmptyHandler, _phantom: PhantomData }
    }
}

impl<Message, P, T> MultipleBuilder<Message, P, T>
where
    Message: Clone,
    P: IntoMessageHandler<Message, T>,
    T: MessageHandler<P::OutMessage>,
{
    pub fn handle<H: MessageHandler<P::OutMessage>>(self, additional_handler: H) -> MultipleBuilder<Message, P, PairHandler<T, H>> {
        let MultipleBuilder { prev, tail, _phantom } = self;
        MultipleBuilder { prev, tail: PairHandler(tail, additional_handler), _phantom }
    }

    pub fn build(self) -> P::Out {
        let MultipleBuilder { prev, tail, .. } = self;
        prev.into_message_handler(tail)
    }
}
