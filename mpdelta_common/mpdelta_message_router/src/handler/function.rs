use crate::static_cow::StaticCow;
use crate::MessageHandler;
use mpdelta_async_runtime::AsyncRuntime;

pub struct FunctionHandler<F>(pub(super) F);

impl<Message, Runtime, F> MessageHandler<Message, Runtime> for FunctionHandler<F>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    F: Fn(Message),
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, _runtime: &Runtime) {
        self.0(message.into_owned())
    }
}
