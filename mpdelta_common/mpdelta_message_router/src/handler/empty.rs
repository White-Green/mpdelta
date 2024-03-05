use crate::handler::{IntoMessageHandler, MessageHandlerBuilder};
use crate::static_cow::StaticCow;
use crate::MessageHandler;
use mpdelta_async_runtime::AsyncRuntime;
use std::marker::PhantomData;

pub struct EmptyHandler;

pub struct EmptyHandlerBuilder<Runtime>(PhantomData<Runtime>);

impl<Runtime> EmptyHandlerBuilder<Runtime> {
    pub(crate) fn new() -> EmptyHandlerBuilder<Runtime> {
        EmptyHandlerBuilder(PhantomData)
    }
}

impl<Message: Clone, Runtime> MessageHandlerBuilder<Message, Runtime> for EmptyHandlerBuilder<Runtime> {
    type OutMessage = Message;
}

impl<Message: Clone, Runtime: AsyncRuntime<()> + Clone, Tail: MessageHandler<Message, Runtime>> IntoMessageHandler<Message, Runtime, Tail> for EmptyHandlerBuilder<Runtime> {
    type Out = Tail;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        tail
    }
}

impl<Message: Clone, Runtime: AsyncRuntime<()> + Clone> MessageHandler<Message, Runtime> for EmptyHandler {
    fn handle<MessageValue: StaticCow<Message>>(&self, _message: MessageValue, _runtime: &Runtime) {}
}
