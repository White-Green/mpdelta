use crate::message_router::static_cow::StaticCow;
use crate::message_router::MessageHandler;
use tokio::runtime::Handle;

pub struct FunctionHandler<F>(pub(super) F);

impl<Message, F> MessageHandler<Message> for FunctionHandler<F>
where
    Message: Clone,
    F: Fn(Message),
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, _runtime: &Handle) {
        self.0(message.into_owned())
    }
}
