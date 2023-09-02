use crate::message_router::handler::{IntoMessageHandler, MessageHandlerBuilder};
use crate::message_router::static_cow::StaticCow;
use crate::message_router::MessageHandler;
use tokio::runtime::Handle;

pub struct EmptyHandler;

pub struct EmptyHandlerBuilder;

impl<Message: Clone> MessageHandlerBuilder<Message> for EmptyHandlerBuilder {
    type OutMessage = Message;
}

impl<Message: Clone, Tail: MessageHandler<Message>> IntoMessageHandler<Message, Tail> for EmptyHandlerBuilder {
    type Out = Tail;

    fn into_message_handler(self, tail: Tail) -> Self::Out {
        tail
    }
}

impl<Message: Clone> MessageHandler<Message> for EmptyHandler {
    fn handle<MessageValue: StaticCow<Message>>(&self, _message: MessageValue, _runtime: &Handle) {}
}
