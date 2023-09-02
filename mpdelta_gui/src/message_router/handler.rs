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
use std::future::IntoFuture;
use std::ops::Deref;
use tokio::runtime::Handle;

pub mod async_function;
pub mod empty;
pub mod filter;
pub mod filter_map;
pub mod function;
pub mod map;
pub mod multiple;
pub mod then;

pub struct PairHandler<HandlerLeft, HandlerRight>(pub(super) HandlerLeft, pub(super) HandlerRight);

impl<Message, HandlerLeft, HandlerRight> MessageHandler<Message> for PairHandler<HandlerLeft, HandlerRight>
where
    Message: Clone,
    HandlerLeft: MessageHandler<Message>,
    HandlerRight: MessageHandler<Message>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Handle) {
        let PairHandler(handler_left, handler_right) = self;
        handler_left.handle(message.clone(), runtime);
        handler_right.handle(message, runtime);
    }
}

pub struct DerefHandler<T>(pub T);

impl<Message, T> MessageHandler<Message> for DerefHandler<T>
where
    T: Deref,
    T::Target: MessageHandler<Message>,
{
    fn handle<MessageValue: StaticCow<Message>>(&self, message: MessageValue, runtime: &Handle) {
        <T as Deref>::deref(&self.0).handle(message, runtime);
    }
}

pub trait MessageHandlerBuilder<Message>: Sized {
    type OutMessage;
    fn filter<F: Fn(&Self::OutMessage) -> bool>(self, f: F) -> FilterBuilder<Self, F> {
        FilterBuilder::new(self, f)
    }

    fn filter_map<O, F: Fn(Self::OutMessage) -> Option<O>>(self, f: F) -> FilterMapBuilder<Self, F> {
        FilterMapBuilder::new(self, f)
    }

    fn map<O, F: Fn(Self::OutMessage) -> O>(self, f: F) -> MapBuilder<Self, F> {
        MapBuilder::new(self, f)
    }

    fn then<Fut: IntoFuture, F: Fn(Self::OutMessage) -> Fut>(self, f: F) -> ThenBuilder<Self, F> {
        ThenBuilder::new(self, f)
    }

    fn multiple(self) -> MultipleBuilder<Message, Self, EmptyHandler> {
        MultipleBuilder::new(self)
    }
}

pub trait IntoMessageHandler<Message, Tail>: MessageHandlerBuilder<Message> {
    type Out: MessageHandler<Message>;
    fn into_message_handler(self, tail: Tail) -> Self::Out;
}

pub trait IntoFunctionHandler<Message: Clone, F>: IntoMessageHandler<Message, FunctionHandler<F>> {
    fn handle(self, f: F) -> Self::Out {
        self.into_message_handler(FunctionHandler(f))
    }
}

impl<T, Message, F> IntoFunctionHandler<Message, F> for T
where
    T: IntoMessageHandler<Message, FunctionHandler<F>>,
    Message: Clone,
    F: Fn(T::OutMessage),
{
}

pub trait IntoAsyncFunctionHandler<Message: Clone, F>: IntoMessageHandler<Message, AsyncFunctionHandler<F>> {
    fn handle_async(self, f: F) -> Self::Out {
        self.into_message_handler(AsyncFunctionHandler::new(f))
    }
}

impl<T, Message, F, Future> IntoAsyncFunctionHandler<Message, F> for T
where
    T: IntoMessageHandler<Message, AsyncFunctionHandler<F>>,
    Message: Clone,
    F: Fn(Self::OutMessage) -> Future,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
{
}

pub trait IntoAsyncFunctionHandlerSingle<Message: Clone, F>: IntoMessageHandler<Message, AsyncFunctionHandlerSingle<Message, F>> {
    fn handle_async_single(self, f: F) -> Self::Out {
        self.into_message_handler(AsyncFunctionHandlerSingle::new(f))
    }
}

impl<T, Message, F, Future> IntoAsyncFunctionHandlerSingle<Message, F> for T
where
    T: IntoMessageHandler<Message, AsyncFunctionHandlerSingle<Message, F>>,
    Message: Clone,
    F: Fn(MessageReceiver<Self::OutMessage>) -> Future,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
{
}

pub trait IntoDerefHandler<Message: Clone, H>: IntoMessageHandler<Message, DerefHandler<H>> {
    fn handle_by(self, f: H) -> Self::Out {
        self.into_message_handler(DerefHandler(f))
    }
}

impl<T, Message, H> IntoDerefHandler<Message, H> for T
where
    T: IntoMessageHandler<Message, DerefHandler<H>>,
    Message: Clone,
    H: Deref,
    H::Target: MessageHandler<Self::OutMessage>,
{
}

pub fn filter<Message, NewF>(condition: NewF) -> FilterBuilder<EmptyHandlerBuilder, NewF>
where
    Message: Clone,
    NewF: Fn(&Message) -> bool,
{
    EmptyHandlerBuilder.filter(condition)
}

pub fn map<Message, NewI, NewF>(new_function: NewF) -> MapBuilder<EmptyHandlerBuilder, NewF>
where
    Message: Clone,
    NewF: Fn(Message) -> NewI,
{
    EmptyHandlerBuilder.map(new_function)
}

pub fn then<Message, Fut, NewF>(new_function: NewF) -> ThenBuilder<EmptyHandlerBuilder, NewF>
where
    Message: Clone,
    Fut: IntoFuture,
    NewF: Fn(Message) -> Fut,
{
    EmptyHandlerBuilder.then(new_function)
}

pub fn filter_map<Message, NewI, NewF>(new_function: NewF) -> FilterMapBuilder<EmptyHandlerBuilder, NewF>
where
    Message: Clone,
    NewF: Fn(Message) -> Option<NewI>,
{
    EmptyHandlerBuilder.filter_map(new_function)
}

pub fn handle<Message, NewF>(new_function: NewF) -> FunctionHandler<NewF>
where
    Message: Clone,
    NewF: Fn(Message),
{
    EmptyHandlerBuilder.handle(new_function)
}

pub fn handle_async<Message, Future, NewF>(new_function: NewF) -> AsyncFunctionHandler<NewF>
where
    Message: Clone,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
    NewF: Fn(Message) -> Future,
{
    EmptyHandlerBuilder.handle_async(new_function)
}

pub fn handle_async_single<Message, Future, NewF>(new_function: NewF) -> AsyncFunctionHandlerSingle<Message, NewF>
where
    Message: Clone,
    Future: IntoFuture<Output = ()>,
    Future::IntoFuture: Send + 'static,
    NewF: Fn(MessageReceiver<Message>) -> Future,
{
    EmptyHandlerBuilder.handle_async_single(new_function)
}

pub fn handle_by<Message, H>(handler: H) -> DerefHandler<H>
where
    Message: Clone,
    H: Deref,
    H::Target: MessageHandler<Message>,
{
    EmptyHandlerBuilder.handle_by(handler)
}
