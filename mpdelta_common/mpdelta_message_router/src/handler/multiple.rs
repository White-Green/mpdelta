use crate::handler::empty::{EmptyHandler, EmptyHandlerBuilder};
use crate::handler::{IntoMessageHandler, MessageHandler, PairHandler};
use std::marker::PhantomData;

pub struct MultipleBuilder<Message, Runtime, P, T> {
    prev: P,
    tail: T,
    _phantom: PhantomData<(Message, Runtime)>,
}

impl<Message, Runtime, P> MultipleBuilder<Message, Runtime, P, EmptyHandler> {
    pub(super) fn new(prev: P) -> MultipleBuilder<Message, Runtime, P, EmptyHandler> {
        MultipleBuilder { prev, tail: EmptyHandler, _phantom: PhantomData }
    }
}

impl<Message, Runtime, P, T> MultipleBuilder<Message, Runtime, P, T>
where
    Message: Clone,
    P: IntoMessageHandler<Message, Runtime, T>,
    T: MessageHandler<P::OutMessage, Runtime>,
{
    pub fn handle<H: MessageHandler<P::OutMessage, Runtime>>(self, additional_handler: impl FnOnce(EmptyHandlerBuilder<Runtime>) -> H) -> MultipleBuilder<Message, Runtime, P, PairHandler<T, H>> {
        let MultipleBuilder { prev, tail, _phantom } = self;
        MultipleBuilder {
            prev,
            tail: PairHandler(tail, additional_handler(EmptyHandlerBuilder::new())),
            _phantom,
        }
    }

    pub fn build(self) -> P::Out {
        let MultipleBuilder { prev, tail, .. } = self;
        prev.into_message_handler(tail)
    }
}
