use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use arc_swap::{ArcSwap, ArcSwapOption};
use crossbeam_utils::atomic::AtomicCell;
use egui::Pos2;
use futures::{stream, FutureExt, StreamExt};
use mpdelta_async_runtime::{AsyncRuntime, JoinHandleWrapper};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::{ComponentInstance, ComponentInstanceId};
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinId, MarkerTime};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::EditEventListener;
use mpdelta_core::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use mpdelta_core::project::{RootComponentClassHandle, RootComponentClassItem};
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::{GetAvailableComponentClassesUsecase, SubscribeEditEventUsecase};
use mpdelta_message_router::handler::{IntoAsyncFunctionHandler, IntoFunctionHandler, MessageHandlerBuilder};
use mpdelta_message_router::{MessageHandler, MessageRouter};
use rpds::HashTrieSet;
use std::collections::HashMap;
use std::hash::Hash;
use std::mem;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::sync::{Mutex, RwLock};

pub struct MarkerPinData<Handle> {
    pub handle: Handle,
    pub at: f64,
    pub locked: bool,
    pub render_location: AtomicCell<Pos2>,
}

impl<Handle> Clone for MarkerPinData<Handle>
where
    Handle: Clone,
{
    fn clone(&self) -> Self {
        let &MarkerPinData { ref handle, at, locked, ref render_location } = self;
        MarkerPinData {
            handle: handle.clone(),
            at,
            locked,
            render_location: AtomicCell::new(render_location.load()),
        }
    }
}

#[derive(Clone)]
pub struct ComponentInstanceData<InstanceHandle, PinHandle> {
    pub handle: InstanceHandle,
    pub name: String,
    pub selected: bool,
    pub start_time: f64,
    pub end_time: f64,
    pub layer: f32,
    pub left_pin: MarkerPinData<PinHandle>,
    pub right_pin: MarkerPinData<PinHandle>,
    pub pins: Vec<MarkerPinData<PinHandle>>,
}

pub type DefaultComponentInstanceData = ComponentInstanceData<ComponentInstanceId, MarkerPinId>;

impl DefaultComponentInstanceData {
    async fn new<T>(component: &ComponentInstance<T>, i: usize, pin_map: &mut HashMap<MarkerPinId, ComponentInstanceId>, root: &RootComponentClassItem<T>) -> DefaultComponentInstanceData
    where
        T: ParameterValueType,
    {
        let start_time = root.time_of_pin(component.marker_left().id()).unwrap().value().into_f64();
        let end_time = root.time_of_pin(component.marker_right().id()).unwrap().value().into_f64();
        pin_map.extend(component.markers().iter().chain([component.marker_left(), component.marker_right()]).map(MarkerPin::id).copied().map(|marker| (marker, *component.id())));
        let left_pin = MarkerPinData {
            handle: *component.marker_left().id(),
            at: root.time_of_pin(component.marker_left().id()).unwrap().value().into_f64(),
            locked: component.marker_left().locked_component_time().is_some(),
            render_location: AtomicCell::new(Pos2::default()),
        };
        let right_pin = MarkerPinData {
            handle: *component.marker_right().id(),
            at: root.time_of_pin(component.marker_right().id()).unwrap().value().into_f64(),
            locked: component.marker_right().locked_component_time().is_some(),
            render_location: AtomicCell::new(Pos2::default()),
        };
        let pins = component
            .markers()
            .iter()
            .map(|pin| MarkerPinData {
                handle: *pin.id(),
                at: root.time_of_pin(pin.id()).unwrap().value().into_f64(),
                locked: pin.locked_component_time().is_some(),
                render_location: AtomicCell::new(Pos2::default()),
            })
            .collect();
        let name = if let Some(component_class) = component.component_class().upgrade() {
            component_class.read().await.human_readable_identifier().to_string()
        } else {
            "** UNKNOWN **".to_string()
        };
        ComponentInstanceData {
            handle: *component.id(),
            name,
            selected: false,
            start_time,
            end_time,
            layer: i as f32,
            left_pin,
            right_pin,
            pins,
        }
    }
}

#[derive(Clone)]
pub struct ComponentInstanceDataList<InstanceHandle, PinHandle> {
    pub list: Vec<ComponentInstanceData<InstanceHandle, PinHandle>>,
}

pub type DefaultComponentInstanceDataList = ComponentInstanceDataList<ComponentInstanceId, MarkerPinId>;

pub struct MarkerLinkData<LinkHandle, PinHandle, ComponentHandle> {
    pub handle: LinkHandle,
    pub len: TimelineTime,
    pub len_str: Mutex<String>,
    pub from_pin: PinHandle,
    pub to_pin: PinHandle,
    pub from_component: Option<ComponentHandle>,
    pub to_component: Option<ComponentHandle>,
    pub from_layer: f32,
    pub to_layer: f32,
    pub from_time: f64,
    pub to_time: f64,
}

pub type DefaultComponentLinkData = MarkerLinkData<MarkerLink, MarkerPinId, ComponentInstanceId>;

impl DefaultComponentLinkData {
    fn new<T: ParameterValueType>(link: MarkerLink, marker_map: &HashMap<MarkerPinId, ComponentInstanceId>, component_map: &HashMap<ComponentInstanceId, DefaultComponentInstanceData>, root: &RootComponentClassItem<T>) -> Option<DefaultComponentLinkData> {
        let from_pin = *link.from();
        let to_pin = *link.to();
        let from_time = root.time_of_pin(link.from()).unwrap().value().into_f64();
        let to_time = root.time_of_pin(link.to()).unwrap().value().into_f64();
        let from_component = marker_map.get(link.from()).cloned();
        let to_component = marker_map.get(link.to()).cloned();
        let from_layer;
        let to_layer;
        match (&from_component, &to_component) {
            (Some(from_component), Some(to_component)) => {
                from_layer = component_map.get(from_component).unwrap().layer;
                to_layer = component_map.get(to_component).unwrap().layer;
            }
            (None, Some(to_component)) => {
                to_layer = component_map.get(to_component).unwrap().layer;
                from_layer = to_layer;
            }
            (Some(from_component), None) => {
                from_layer = component_map.get(from_component).unwrap().layer;
                to_layer = from_layer;
            }
            (None, None) => {
                from_layer = 0.0;
                to_layer = 0.0;
            }
        }
        let len = link.len();
        Some(MarkerLinkData {
            handle: link.clone(),
            len,
            len_str: Mutex::new(len.value().to_string()),
            from_pin,
            to_pin,
            from_component,
            to_component,
            from_layer,
            to_layer,
            from_time,
            to_time,
        })
    }
}

pub struct MarkerLinkDataList<LinkHandle, PinHandle, ComponentHandle> {
    pub list: Vec<MarkerLinkData<LinkHandle, PinHandle, ComponentHandle>>,
}

pub type DefaultComponentLinkDataList = MarkerLinkDataList<MarkerLink, MarkerPinId, ComponentInstanceId>;

pub struct ComponentClassData<Handle> {
    pub name: String,
    pub handle: Handle,
}

impl<T> ComponentClassData<StaticPointer<RwLock<dyn ComponentClass<T>>>>
where
    T: ParameterValueType,
{
    async fn new(handle: StaticPointer<RwLock<dyn ComponentClass<T>>>) -> ComponentClassData<StaticPointer<RwLock<dyn ComponentClass<T>>>> {
        let name = if let Some(handle) = handle.upgrade() { handle.read().await.human_readable_identifier().to_string() } else { "** UNKNOWN **".to_string() };
        ComponentClassData { name, handle }
    }
}

pub struct ComponentClassDataList<Handle> {
    pub list: Vec<ComponentClassData<Handle>>,
}

pub type DefaultComponentClassDataList<T> = ComponentClassDataList<StaticPointer<RwLock<dyn ComponentClass<T>>>>;

pub trait TimelineViewModel<T: ParameterValueType> {
    fn component_length(&self) -> Option<MarkerTime>;
    fn seek(&self) -> MarkerTime;
    fn set_seek(&self, seek: MarkerTime);
    fn edit_component_length(&self, length: MarkerTime);
    type ComponentInstanceHandle: Clone + Eq + Hash;
    type MarkerPinHandle: Clone + Eq + Hash;
    fn component_instances<R>(&self, f: impl FnOnce(&ComponentInstanceDataList<Self::ComponentInstanceHandle, Self::MarkerPinHandle>) -> R) -> R;
    fn click_component_instance(&self, handle: &Self::ComponentInstanceHandle);
    fn delete_component_instance(&self, handle: &Self::ComponentInstanceHandle);
    fn move_component_instance(&self, handle: &Self::ComponentInstanceHandle, to: f64);
    fn insert_component_instance_to(&self, handle: &Self::ComponentInstanceHandle, index: usize);
    fn move_marker_pin(&self, instance_handle: &Self::ComponentInstanceHandle, pin_handle: &Self::MarkerPinHandle, to: f64);
    fn connect_marker_pins(&self, from: &Self::MarkerPinHandle, to: &Self::MarkerPinHandle);
    fn add_marker_pin(&self, instance: &Self::ComponentInstanceHandle, at: TimelineTime);
    fn delete_marker_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle);
    fn lock_marker_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle);
    fn unlock_marker_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle);
    fn split_component_at_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle);
    type MarkerLinkHandle: Clone + Eq + Hash;
    fn marker_links<R>(&self, f: impl FnOnce(&MarkerLinkDataList<Self::MarkerLinkHandle, Self::MarkerPinHandle, Self::ComponentInstanceHandle>) -> R) -> R;
    fn edit_marker_link_length(&self, link: &Self::MarkerLinkHandle, value: f64);
    type ComponentClassHandle: Clone + Eq + Hash;
    fn component_classes<R>(&self, f: impl FnOnce(&ComponentClassDataList<Self::ComponentClassHandle>) -> R) -> R;
    fn add_component_instance(&self, class: Self::ComponentClassHandle);
}

pub struct TimelineViewModelImpl<T: ParameterValueType, GlobalUIState, MessageHandler, G, Runtime, JoinHandle> {
    global_ui_state: Arc<GlobalUIState>,
    component_classes: Arc<ArcSwap<DefaultComponentClassDataList<T>>>,
    component_instances: Arc<ArcSwap<DefaultComponentInstanceDataList>>,
    marker_links: Arc<ArcSwap<DefaultComponentLinkDataList>>,
    selected_root_component_class: Arc<ArcSwapOption<RootComponentClassHandle<T>>>,
    message_router: MessageRouter<MessageHandler, Runtime>,
    runtime: Runtime,
    load_timeline_task: Arc<StdMutex<Option<JoinHandleWrapper<JoinHandle>>>>,
    guard: OnceLock<G>,
}

pub enum Message<T: ParameterValueType> {
    GlobalUIEvent(GlobalUIEvent<T>),
    AddComponentInstance(StaticPointer<RwLock<dyn ComponentClass<T>>>),
    ClickComponentInstance(ComponentInstanceId),
    DeleteComponentInstance(ComponentInstanceId),
    MoveComponentInstance(ComponentInstanceId, f64),
    InsertComponentInstanceTo(ComponentInstanceId, usize),
    MoveMarkerPin(ComponentInstanceId, MarkerPinId, f64),
    ConnectMarkerPins(MarkerPinId, MarkerPinId),
    EditMarkerLinkLength(MarkerLink, f64),
    EditComponentLength(MarkerTime),
    AddMarkerPin(ComponentInstanceId, TimelineTime),
    DeleteMarkerPin(ComponentInstanceId, MarkerPinId),
    LockMarkerPin(ComponentInstanceId, MarkerPinId),
    UnlockMarkerPin(ComponentInstanceId, MarkerPinId),
    SplitComponentAtPin(ComponentInstanceId, MarkerPinId),
}

impl<T> Clone for Message<T>
where
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        match self {
            Message::GlobalUIEvent(value) => Message::GlobalUIEvent(value.clone()),
            Message::AddComponentInstance(value) => Message::AddComponentInstance(value.clone()),
            Message::ClickComponentInstance(value) => Message::ClickComponentInstance(*value),
            Message::DeleteComponentInstance(value) => Message::DeleteComponentInstance(*value),
            &Message::MoveComponentInstance(ref value, to) => Message::MoveComponentInstance(*value, to),
            &Message::InsertComponentInstanceTo(ref instance, index) => Message::InsertComponentInstanceTo(*instance, index),
            &Message::MoveMarkerPin(ref instance, ref pin, to) => Message::MoveMarkerPin(*instance, *pin, to),
            Message::ConnectMarkerPins(from, to) => Message::ConnectMarkerPins(*from, *to),
            &Message::EditMarkerLinkLength(ref value, length) => Message::EditMarkerLinkLength(value.clone(), length),
            &Message::EditComponentLength(value) => Message::EditComponentLength(value),
            &Message::AddMarkerPin(ref instance, at) => Message::AddMarkerPin(*instance, at),
            Message::DeleteMarkerPin(instance, pin) => Message::DeleteMarkerPin(*instance, *pin),
            Message::LockMarkerPin(instance, pin) => Message::LockMarkerPin(*instance, *pin),
            Message::UnlockMarkerPin(instance, pin) => Message::UnlockMarkerPin(*instance, *pin),
            Message::SplitComponentAtPin(instance, pin) => Message::SplitComponentAtPin(*instance, *pin),
        }
    }
}

impl<T> PartialEq for Message<T>
where
    T: ParameterValueType,
{
    fn eq(&self, other: &Self) -> bool {
        if mem::discriminant(self) != mem::discriminant(other) {
            return false;
        }
        match (self, other) {
            (Message::GlobalUIEvent(a), Message::GlobalUIEvent(b)) => a == b,
            (Message::AddComponentInstance(a), Message::AddComponentInstance(b)) => a == b,
            (Message::ClickComponentInstance(a), Message::ClickComponentInstance(b)) => a == b,
            (Message::DeleteComponentInstance(a), Message::DeleteComponentInstance(b)) => a == b,
            (Message::MoveComponentInstance(a, at), Message::MoveComponentInstance(b, bt)) => a == b && at == bt,
            (Message::InsertComponentInstanceTo(a, ai), Message::InsertComponentInstanceTo(b, bi)) => a == b && ai == bi,
            (Message::MoveMarkerPin(ai, ap, at), Message::MoveMarkerPin(bi, bp, bt)) => ai == bi && ap == bp && at == bt,
            (Message::ConnectMarkerPins(a, b), Message::ConnectMarkerPins(c, d)) => a == c && b == d,
            (Message::EditMarkerLinkLength(a, al), Message::EditMarkerLinkLength(b, bl)) => a == b && al == bl,
            (Message::EditComponentLength(a), Message::EditComponentLength(b)) => a == b,
            (Message::AddMarkerPin(a, at), Message::AddMarkerPin(b, bt)) => a == b && at == bt,
            (Message::DeleteMarkerPin(a, ap), Message::DeleteMarkerPin(b, bp)) => a == b && ap == bp,
            (Message::LockMarkerPin(a, ap), Message::LockMarkerPin(b, bp)) => a == b && ap == bp,
            (Message::UnlockMarkerPin(a, ap), Message::UnlockMarkerPin(b, bp)) => a == b && ap == bp,
            (Message::SplitComponentAtPin(a, ap), Message::SplitComponentAtPin(b, bp)) => a == b && ap == bp,
            _ => unreachable!(),
        }
    }
}

impl<T> Eq for Message<T> where T: ParameterValueType {}

impl<T, S, M, G, Runtime> GlobalUIEventHandler<T> for TimelineViewModelImpl<T, S, M, G, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    S: GlobalUIState<T>,
    M: MessageHandler<Message<T>, Runtime>,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn handle(&self, event: GlobalUIEvent<T>) {
        self.message_router.handle(Message::GlobalUIEvent(event));
    }
}

impl<T, S, M, G, Runtime> EditEventListener<T> for TimelineViewModelImpl<T, S, M, G, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    S: GlobalUIState<T>,
    M: MessageHandler<Message<T>, Runtime> + Send + Sync,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn on_edit(&self, _: &RootComponentClassHandle<T>, _: RootComponentEditEvent) {
        use_arc!(component_instances = self.component_instances, marker_links = self.marker_links, selected_root_component_class = self.selected_root_component_class);
        let mut task = self.load_timeline_task.lock().unwrap();
        let future = TimelineViewModelImpl::load_timeline_by_current_root_component_class(component_instances, marker_links, selected_root_component_class);
        if let Some(handle) = task.take() {
            handle.abort();
            *task = Some(self.runtime.spawn(handle.then(|_| future)));
        } else {
            *task = Some(self.runtime.spawn(future));
        }
    }

    fn on_edit_instance(&self, _: &RootComponentClassHandle<T>, _: &ComponentInstanceId, _: InstanceEditEvent<T>) {
        use_arc!(component_instances = self.component_instances, marker_links = self.marker_links, selected_root_component_class = self.selected_root_component_class);
        let mut task = self.load_timeline_task.lock().unwrap();
        let future = TimelineViewModelImpl::load_timeline_by_current_root_component_class(component_instances, marker_links, selected_root_component_class);
        if let Some(handle) = task.take() {
            handle.abort();
            *task = Some(self.runtime.spawn(handle.then(|_| future)));
        } else {
            *task = Some(self.runtime.spawn(future));
        }
    }
}

impl<T: ParameterValueType> TimelineViewModelImpl<T, (), (), (), (), ()> {
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<T>, Edit: EditFunnel<T> + 'static, P: ViewModelParams<T>>(
        global_ui_state: &Arc<S>,
        edit: &Arc<Edit>,
        params: &P,
    ) -> Arc<TimelineViewModelImpl<T, S, impl MessageHandler<Message<T>, P::AsyncRuntime>, <P::SubscribeEditEvent as SubscribeEditEventUsecase<T>>::EditEventListenerGuard, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>> {
        let component_classes = Arc::new(ArcSwap::new(Arc::new(ComponentClassDataList { list: Vec::new() })));
        let selected_components = Arc::new(ArcSwap::new(Arc::new(HashTrieSet::new_sync())));
        let marker_links = Arc::new(ArcSwap::new(Arc::new(MarkerLinkDataList { list: Vec::new() })));
        let component_instances = Arc::new(ArcSwap::new(Arc::new(ComponentInstanceDataList { list: Vec::new() })));
        let selected_root_component_class = Arc::new(ArcSwapOption::new(None));
        let load_timeline_task = Arc::new(StdMutex::new(None::<JoinHandleWrapper<<P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>>));
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler
                    .filter_map(|message| if let Message::GlobalUIEvent(value) = message { Some(value) } else { None })
                    .multiple()
                    .handle(|handler| {
                        handler.filter_map(|event| if let GlobalUIEvent::SelectRootComponentClass(value) = event { Some(value) } else { None }).handle({
                            let runtime = params.runtime().clone();
                            use_arc!(selected_root_component_class, component_instances, marker_links, load_timeline_task);
                            move |root_component_class| {
                                use_arc!(selected_root_component_class, component_instances, marker_links);
                                let mut task = load_timeline_task.lock().unwrap();
                                let future = Self::load_timeline_by_new_root_component_class(root_component_class, component_instances, marker_links, selected_root_component_class);
                                if let Some(handle) = task.take() {
                                    handle.abort();
                                    *task = Some(runtime.spawn(handle.then(|_| future)));
                                } else {
                                    *task = Some(runtime.spawn(future));
                                }
                            }
                        })
                    })
                    .build()
            })
            .handle(|handler| {
                handler
                    .filter(|message| {
                        matches!(
                            message,
                            Message::EditComponentLength(_) | Message::AddComponentInstance(_) | Message::DeleteComponentInstance(_) | Message::EditMarkerLinkLength(_, _) | Message::InsertComponentInstanceTo(_, _) | Message::ConnectMarkerPins(_, _)
                        )
                    })
                    .handle_async({
                        use_arc!(selected_root_component_class, edit, id = params.id_generator());
                        move |message| {
                            use_arc!(selected_root_component_class, edit, id);
                            async move {
                                let selected_root_component_class = selected_root_component_class.load();
                                let Some(target) = selected_root_component_class.as_deref() else {
                                    return;
                                };
                                let command = match message {
                                    Message::AddComponentInstance(pointer) => {
                                        let class = pointer.upgrade().unwrap();
                                        let instance = class.read().await.instantiate(&pointer, &id).await;
                                        RootComponentEditCommand::AddComponentInstance(instance)
                                    }
                                    Message::DeleteComponentInstance(handle) => RootComponentEditCommand::DeleteComponentInstance(handle),
                                    Message::EditMarkerLinkLength(target, len) => RootComponentEditCommand::EditMarkerLinkLength(target, TimelineTime::new(MixedFraction::from_f64(len))),
                                    Message::EditComponentLength(len) => RootComponentEditCommand::EditComponentLength(len),
                                    Message::InsertComponentInstanceTo(handle, index) => RootComponentEditCommand::InsertComponentInstanceTo(handle, index),
                                    Message::ConnectMarkerPins(from, to) => RootComponentEditCommand::ConnectMarkerPins(from, to),
                                    _ => unreachable!(),
                                };
                                edit.edit(target, command);
                            }
                        }
                    })
            })
            .handle(|handler| {
                handler.filter_map(|message| if let Message::ClickComponentInstance(value) = message { Some(value) } else { None }).handle_async({
                    use_arc!(selected_components, component_instances, global_ui_state);
                    move |target| {
                        global_ui_state.select_component_instance(&target);
                        use_arc!(selected_components, component_instances);
                        async move {
                            let mut component_instances_inner = ComponentInstanceDataList::clone(&component_instances.load());
                            component_instances_inner.list.iter_mut().for_each(|ComponentInstanceData { handle, selected, .. }| *selected = *handle == target);
                            component_instances.store(Arc::new(component_instances_inner));
                            selected_components.store(Arc::new(HashTrieSet::from_iter([target])));
                        }
                    }
                })
            })
            .handle(|handler| {
                handler
                    .filter(|message| {
                        matches!(
                            message,
                            Message::MoveComponentInstance(_, _) | Message::MoveMarkerPin(_, _, _) | Message::AddMarkerPin(_, _) | Message::DeleteMarkerPin(_, _) | Message::LockMarkerPin(_, _) | Message::UnlockMarkerPin(_, _) | Message::SplitComponentAtPin(_, _)
                        )
                    })
                    .handle_async({
                        use_arc!(selected_root_component_class, edit);
                        move |message| {
                            use_arc!(selected_root_component_class, edit);
                            async move {
                                let selected_root_component_class = selected_root_component_class.load();
                                let Some(target_root) = selected_root_component_class.as_deref() else {
                                    return;
                                };
                                let (target, command) = match message {
                                    Message::MoveComponentInstance(target, to) => (target, InstanceEditCommand::MoveComponentInstance(TimelineTime::new(MixedFraction::from_f64(to)))),
                                    Message::MoveMarkerPin(target, pin, to) => (target, InstanceEditCommand::MoveMarkerPin(pin, TimelineTime::new(MixedFraction::from_f64(to)))),
                                    Message::AddMarkerPin(target, at) => (target, InstanceEditCommand::AddMarkerPin(at)),
                                    Message::DeleteMarkerPin(target, pin) => (target, InstanceEditCommand::DeleteMarkerPin(pin)),
                                    Message::LockMarkerPin(target, pin) => (target, InstanceEditCommand::LockMarkerPin(pin)),
                                    Message::UnlockMarkerPin(target, pin) => (target, InstanceEditCommand::UnlockMarkerPin(pin)),
                                    Message::SplitComponentAtPin(target, pin) => (target, InstanceEditCommand::SplitAtPin(pin)),
                                    _ => unreachable!(),
                                };
                                edit.edit_instance(target_root, &target, command);
                            }
                        }
                    })
            })
            .build(params.runtime().clone());
        params.runtime().spawn({
            use_arc!(component_classes, get_available_component_classes = params.get_available_component_classes());
            async move {
                let available_component_classes = get_available_component_classes.get_available_component_classes().await;
                component_classes.store(Arc::new(ComponentClassDataList {
                    list: stream::iter(available_component_classes.iter().cloned()).then(ComponentClassData::new).collect().await,
                }));
            }
        });
        let arc = Arc::new(TimelineViewModelImpl {
            global_ui_state: Arc::clone(global_ui_state),
            component_classes,
            component_instances,
            marker_links,
            selected_root_component_class,
            message_router,
            runtime: params.runtime().clone(),
            load_timeline_task,
            guard: OnceLock::new(),
        });
        global_ui_state.register_global_ui_event_handler(Arc::clone(&arc));
        let guard = params.subscribe_edit_event().add_edit_event_listener(Arc::clone(&arc));
        arc.guard.set(guard).unwrap_or_else(|_| unreachable!());
        arc
    }

    async fn load_timeline_by_new_root_component_class(
        root_component_class: Option<RootComponentClassHandle<T>>,
        component_instances: Arc<ArcSwap<DefaultComponentInstanceDataList>>,
        marker_links: Arc<ArcSwap<DefaultComponentLinkDataList>>,
        selected_root_component_class: Arc<ArcSwapOption<RootComponentClassHandle<T>>>,
    ) {
        selected_root_component_class.store(root_component_class.clone().map(Arc::new));
        Self::load_timeline_inner(root_component_class.as_ref(), &component_instances, &marker_links).await;
    }

    async fn load_timeline_by_current_root_component_class(component_instances: Arc<ArcSwap<DefaultComponentInstanceDataList>>, marker_links: Arc<ArcSwap<DefaultComponentLinkDataList>>, selected_root_component_class: Arc<ArcSwapOption<RootComponentClassHandle<T>>>) {
        Self::load_timeline_inner(selected_root_component_class.load().as_deref(), &component_instances, &marker_links).await;
    }

    async fn load_timeline_inner(root_component_class: Option<&RootComponentClassHandle<T>>, component_instances: &ArcSwap<DefaultComponentInstanceDataList>, marker_links: &ArcSwap<DefaultComponentLinkDataList>) {
        let Some(root_component_class) = root_component_class else {
            return;
        };
        let Some(root_component_class) = root_component_class.upgrade() else {
            return;
        };
        let root_component_class = root_component_class.read().await;
        let root_component_class = root_component_class.get();
        let mut pin_map = HashMap::new();
        let mut list = Vec::new();
        for (i, handle) in root_component_class.iter_components().enumerate() {
            let component_instance = ComponentInstanceData::new(handle, i, &mut pin_map, &root_component_class).await;
            list.push(component_instance);
        }
        let component_instances_inner = ComponentInstanceDataList { list };
        let component_map = component_instances_inner.list.iter().cloned().map(|component| (component.handle, component)).collect();
        let list = root_component_class.iter_links().filter_map(|handle| MarkerLinkData::new(handle.clone(), &pin_map, &component_map, &root_component_class)).collect();
        let links = MarkerLinkDataList { list };
        component_instances.store(Arc::new(component_instances_inner));
        marker_links.store(Arc::new(links));
    }
}

impl<T, S, M, G, Runtime> TimelineViewModel<T> for TimelineViewModelImpl<T, S, M, G, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    S: GlobalUIState<T>,
    M: MessageHandler<Message<T>, Runtime>,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn component_length(&self) -> Option<MarkerTime> {
        self.global_ui_state.component_length()
    }
    fn seek(&self) -> MarkerTime {
        self.global_ui_state.seek()
    }
    fn set_seek(&self, seek: MarkerTime) {
        if !self.global_ui_state.playing() {
            self.global_ui_state.set_seek(seek.min(self.global_ui_state.component_length().unwrap_or_else(|| MarkerTime::new(MixedFraction::from_integer(10)).unwrap())));
        }
    }

    fn edit_component_length(&self, length: MarkerTime) {
        if !self.global_ui_state.playing() {
            if self.global_ui_state.seek() > length {
                self.set_seek(length);
            }
            self.message_router.handle(Message::EditComponentLength(length));
        }
    }

    type ComponentInstanceHandle = ComponentInstanceId;

    type MarkerPinHandle = MarkerPinId;

    fn component_instances<R>(&self, f: impl FnOnce(&ComponentInstanceDataList<Self::ComponentInstanceHandle, Self::MarkerPinHandle>) -> R) -> R {
        f(&self.component_instances.load())
    }

    fn click_component_instance(&self, handle: &Self::ComponentInstanceHandle) {
        self.message_router.handle(Message::ClickComponentInstance(*handle));
    }

    fn delete_component_instance(&self, handle: &Self::ComponentInstanceHandle) {
        self.message_router.handle(Message::DeleteComponentInstance(*handle));
    }

    fn move_component_instance(&self, handle: &Self::ComponentInstanceHandle, to: f64) {
        self.message_router.handle(Message::MoveComponentInstance(*handle, to));
    }

    fn insert_component_instance_to(&self, handle: &Self::ComponentInstanceHandle, index: usize) {
        self.message_router.handle(Message::InsertComponentInstanceTo(*handle, index));
    }

    fn move_marker_pin(&self, instance_handle: &Self::ComponentInstanceHandle, pin_handle: &Self::MarkerPinHandle, to: f64) {
        self.message_router.handle(Message::MoveMarkerPin(*instance_handle, *pin_handle, to));
    }

    fn connect_marker_pins(&self, from: &Self::MarkerPinHandle, to: &Self::MarkerPinHandle) {
        self.message_router.handle(Message::ConnectMarkerPins(*from, *to));
    }

    fn add_marker_pin(&self, instance: &Self::ComponentInstanceHandle, at: TimelineTime) {
        self.message_router.handle(Message::AddMarkerPin(*instance, at));
    }

    fn delete_marker_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle) {
        self.message_router.handle(Message::DeleteMarkerPin(*instance, *pin));
    }

    fn lock_marker_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle) {
        self.message_router.handle(Message::LockMarkerPin(*instance, *pin));
    }

    fn unlock_marker_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle) {
        self.message_router.handle(Message::UnlockMarkerPin(*instance, *pin));
    }

    fn split_component_at_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle) {
        self.message_router.handle(Message::SplitComponentAtPin(*instance, *pin))
    }

    type MarkerLinkHandle = MarkerLink;

    fn marker_links<R>(&self, f: impl FnOnce(&MarkerLinkDataList<Self::MarkerLinkHandle, Self::MarkerPinHandle, Self::ComponentInstanceHandle>) -> R) -> R {
        f(&self.marker_links.load())
    }

    fn edit_marker_link_length(&self, link: &Self::MarkerLinkHandle, value: f64) {
        self.message_router.handle(Message::EditMarkerLinkLength(link.clone(), value));
    }

    type ComponentClassHandle = StaticPointer<RwLock<dyn ComponentClass<T>>>;

    fn component_classes<R>(&self, f: impl FnOnce(&ComponentClassDataList<Self::ComponentClassHandle>) -> R) -> R {
        f(&self.component_classes.load())
    }

    fn add_component_instance(&self, class: Self::ComponentClassHandle) {
        self.message_router.handle(Message::AddComponentInstance(class));
    }
}
