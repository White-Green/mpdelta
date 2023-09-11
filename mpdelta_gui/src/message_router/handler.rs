use crate::message_router::handler::async_function::{AsyncFunctionHandler, AsyncFunctionHandlerSingle, MessageReceiver};
use crate::message_router::handler::empty::{EmptyHandler, EmptyHandlerBuilder};
use crate::message_router::handler::filter_map::FilterMapBuilder;
use crate::message_router::handler::function::FunctionHandler;
use crate::message_router::handler::map::MapBuilder;
use crate::message_router::handler::multiple::MultipleBuilder;
use crate::message_router::handler::then::ThenBuilder;
use crate::message_router::static_cow::StaticCow;
use crate::message_router::MessageHandler;
use filter::FilterBuilder;
use mpdelta_async_runtime::AsyncRuntime;
use std::future::IntoFuture;
use std::ops::Deref;

pub mod async_function;
pub mod empty;
pub mod filter;
pub mod filter_map;
pub mod function;
pub mod map;
pub mod multiple;
pub mod then;

pub struct PairHandler<HandlerLeft, HandlerRight>(pub(super) HandlerLeft, pub(super) HandlerRight);

impl<Message, Runtime, HandlerLeft, HandlerRight> MessageHandler<Message, Runtime> for PairHandler<HandlerLeft, HandlerRight>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    HandlerLeft: MessageHandler<Message, Runtime>,
    HandlerRight: MessageHandler<Message, Runtime>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
        let PairHandler(handler_left, handler_right) = self;
        handler_left.handle(message.clone(), runtime);
        handler_right.handle(message, runtime);
    }
}

pub struct DerefHandler<T>(pub T);

impl<Message, Runtime, T> MessageHandler<Message, Runtime> for DerefHandler<T>
where
    Runtime: AsyncRuntime<()> + Clone,
    T: Deref,
    T::Target: MessageHandler<Message, Runtime>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Runtime) {
        <T as Deref>::deref(&self.0).handle(message, runtime);
    }
}

pub trait MessageHandlerBuilder<Message, Runtime>: Sized {
    type OutMessage;
    fn filter<F: Fn(&Self::OutMessage) -> bool>(self, f: F) -> FilterBuilder<Self, F, Runtime> {
        FilterBuilder::new(self, f)
    }

    fn filter_map<O, F: Fn(Self::OutMessage) -> Option<O>>(self, f: F) -> FilterMapBuilder<Self, F, Runtime> {
        FilterMapBuilder::new(self, f)
    }

    fn map<O, F: Fn(Self::OutMessage) -> O>(self, f: F) -> MapBuilder<Self, F, Runtime> {
        MapBuilder::new(self, f)
    }

    fn then<Fut: IntoFuture, F: Fn(Self::OutMessage) -> Fut>(self, f: F) -> ThenBuilder<Self, F, Runtime> {
        ThenBuilder::new(self, f)
    }

    fn multiple(self) -> MultipleBuilder<Message, Runtime, Self, EmptyHandler> {
        MultipleBuilder::new(self)
    }
}

pub trait IntoMessageHandler<Message, Runtime, Tail>: MessageHandlerBuilder<Message, Runtime> {
    type Out: MessageHandler<Message, Runtime>;
    fn into_message_handler(self, tail: Tail) -> Self::Out;
}

pub trait IntoFunctionHandler<Message: Clone, Runtime, F>: IntoMessageHandler<Message, Runtime, FunctionHandler<F>> {
    fn handle(self, f: F) -> Self::Out {
        self.into_message_handler(FunctionHandler(f))
    }
}

impl<T, Message, Runtime, F> IntoFunctionHandler<Message, Runtime, F> for T
where
    T: IntoMessageHandler<Message, Runtime, FunctionHandler<F>>,
    Message: Clone,
    F: Fn(T::OutMessage),
{
}

pub trait IntoAsyncFunctionHandler<Message: Clone, Runtime, F>: IntoMessageHandler<Message, Runtime, AsyncFunctionHandler<F>> {
    fn handle_async(self, f: F) -> Self::Out {
        self.into_message_handler(AsyncFunctionHandler::new(f))
    }
}

impl<T, Message, Runtime, F, Future> IntoAsyncFunctionHandler<Message, Runtime, F> for T
where
    T: IntoMessageHandler<Message, Runtime, AsyncFunctionHandler<F>>,
    Message: Clone,
    F: Fn(Self::OutMessage) -> Future,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
{
}

pub trait IntoAsyncFunctionHandlerSingle<Message: Clone, Runtime: AsyncRuntime<()>, F>: IntoMessageHandler<Message, Runtime, AsyncFunctionHandlerSingle<Message, F, Runtime::JoinHandle>> {
    fn handle_async_single(self, f: F) -> Self::Out {
        self.into_message_handler(AsyncFunctionHandlerSingle::new::<Runtime>(f))
    }
}

impl<T, Message, Runtime, F, Future> IntoAsyncFunctionHandlerSingle<Message, Runtime, F> for T
where
    T: IntoMessageHandler<Message, Runtime, AsyncFunctionHandlerSingle<Message, F, Runtime::JoinHandle>>,
    Message: Clone,
    Runtime: AsyncRuntime<()>,
    F: Fn(MessageReceiver<Self::OutMessage>) -> Future,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
{
}

pub trait IntoDerefHandler<Message: Clone, Runtime, H>: IntoMessageHandler<Message, Runtime, DerefHandler<H>> {
    fn handle_by(self, f: H) -> Self::Out {
        self.into_message_handler(DerefHandler(f))
    }
}

impl<T, Message, Runtime, H> IntoDerefHandler<Message, Runtime, H> for T
where
    T: IntoMessageHandler<Message, Runtime, DerefHandler<H>>,
    Message: Clone,
    H: Deref,
    H::Target: MessageHandler<Self::OutMessage, Runtime>,
{
}

pub fn filter<Message, Runtime, NewF>(condition: NewF) -> FilterBuilder<EmptyHandlerBuilder<Runtime>, NewF, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    NewF: Fn(&Message) -> bool,
{
    EmptyHandlerBuilder::<Runtime>::new().filter(condition)
}

pub fn map<Message, Runtime, NewI, NewF>(new_function: NewF) -> MapBuilder<EmptyHandlerBuilder<Runtime>, NewF, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    NewF: Fn(Message) -> NewI,
{
    EmptyHandlerBuilder::<Runtime>::new().map(new_function)
}

pub fn then<Message, Runtime, Fut, NewF>(new_function: NewF) -> ThenBuilder<EmptyHandlerBuilder<Runtime>, NewF, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Fut: IntoFuture,
    NewF: Fn(Message) -> Fut,
{
    EmptyHandlerBuilder::<Runtime>::new().then(new_function)
}

pub fn filter_map<Message, Runtime, NewI, NewF>(new_function: NewF) -> FilterMapBuilder<EmptyHandlerBuilder<Runtime>, NewF, Runtime>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    NewF: Fn(Message) -> Option<NewI>,
{
    EmptyHandlerBuilder::<Runtime>::new().filter_map(new_function)
}

pub fn handle<Message, Runtime, NewF>(new_function: NewF) -> FunctionHandler<NewF>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    NewF: Fn(Message),
{
    EmptyHandlerBuilder::<Runtime>::new().handle(new_function)
}

pub fn handle_async<Message, Runtime, Future, NewF>(new_function: NewF) -> AsyncFunctionHandler<NewF>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
    NewF: Fn(Message) -> Future,
{
    EmptyHandlerBuilder::<Runtime>::new().handle_async(new_function)
}

pub fn handle_async_single<Message, Runtime, Future, NewF>(new_function: NewF) -> AsyncFunctionHandlerSingle<Message, NewF, Runtime::JoinHandle>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
    NewF: Fn(MessageReceiver<Message>) -> Future,
{
    EmptyHandlerBuilder::<Runtime>::new().handle_async_single(new_function)
}

pub fn handle_by<Message, Runtime, H>(handler: H) -> DerefHandler<H>
where
    Message: Clone,
    Runtime: AsyncRuntime<()> + Clone,
    H: Deref,
    H::Target: MessageHandler<Message, Runtime>,
{
    EmptyHandlerBuilder::<Runtime>::new().handle_by(handler)
}
