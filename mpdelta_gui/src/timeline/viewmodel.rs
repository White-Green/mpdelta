use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use arc_swap::ArcSwap;
use egui::epaint::ahash::{HashSet, HashSetExt};
use egui::Pos2;
use futures::FutureExt;
use mpdelta_async_runtime::{AsyncRuntime, JoinHandleWrapper};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::MarkerLinkHandle;
use mpdelta_core::component::marker_pin::{MarkerPinHandle, MarkerTime};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::EditEventListener;
use mpdelta_core::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::{GetAvailableComponentClassesUsecase, SubscribeEditEventUsecase};
use mpdelta_message_router::handler::{IntoAsyncFunctionHandler, IntoFunctionHandler, MessageHandlerBuilder};
use mpdelta_message_router::{MessageHandler, MessageRouter};
use qcell::{TCell, TCellOwner};
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::Hash;
use std::mem;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct MarkerPinData<Handle> {
    pub handle: Handle,
    pub at: f64,
    pub render_location: Cell<Pos2>,
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

pub type DefaultComponentInstanceData<K, T> = ComponentInstanceData<ComponentInstanceHandle<K, T>, MarkerPinHandle<K>>;

impl<K, T> DefaultComponentInstanceData<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn new(handle: ComponentInstanceHandle<K, T>, key: &TCellOwner<K>, i: usize, pin_map: &mut HashMap<MarkerPinHandle<K>, ComponentInstanceHandle<K, T>>) -> Option<DefaultComponentInstanceData<K, T>> {
        let component = handle.upgrade()?;
        let component = component.ro(key);
        let start_time = component.marker_left().ro(key).cached_timeline_time().value().into_f64();
        let end_time = component.marker_right().ro(key).cached_timeline_time().value().into_f64();
        pin_map.extend(component.markers().iter().chain([component.marker_left(), component.marker_right()]).map(StaticPointerOwned::reference).cloned().map(|marker| (marker, handle.clone())));
        let left_pin = MarkerPinData {
            handle: StaticPointerOwned::reference(component.marker_left()).clone(),
            at: component.marker_left().ro(key).cached_timeline_time().value().into_f64(),
            render_location: Cell::new(Pos2::default()),
        };
        let right_pin = MarkerPinData {
            handle: StaticPointerOwned::reference(component.marker_right()).clone(),
            at: component.marker_right().ro(key).cached_timeline_time().value().into_f64(),
            render_location: Cell::new(Pos2::default()),
        };
        let pins = component
            .markers()
            .iter()
            .map(|pin| MarkerPinData {
                handle: StaticPointerOwned::reference(pin).clone(),
                at: pin.ro(key).cached_timeline_time().value().into_f64(),
                render_location: Cell::new(Pos2::default()),
            })
            .collect();
        Some(ComponentInstanceData {
            handle,
            name: "TEST".to_string(),
            selected: false,
            start_time,
            end_time,
            layer: i as f32,
            left_pin,
            right_pin,
            pins,
        })
    }
}

pub struct ComponentInstanceDataList<InstanceHandle, PinHandle> {
    pub list: Vec<ComponentInstanceData<InstanceHandle, PinHandle>>,
}

pub type DefaultComponentInstanceDataList<K, T> = ComponentInstanceDataList<ComponentInstanceHandle<K, T>, MarkerPinHandle<K>>;

pub struct ComponentLinkData<LinkHandle, PinHandle, ComponentHandle> {
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

pub type DefaultComponentLinkData<K, T> = ComponentLinkData<MarkerLinkHandle<K>, MarkerPinHandle<K>, ComponentInstanceHandle<K, T>>;

impl<K, T> DefaultComponentLinkData<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn new(handle: MarkerLinkHandle<K>, key: &TCellOwner<K>, marker_map: &HashMap<MarkerPinHandle<K>, ComponentInstanceHandle<K, T>>, component_map: &HashMap<ComponentInstanceHandle<K, T>, DefaultComponentInstanceData<K, T>>) -> Option<DefaultComponentLinkData<K, T>> {
        let Some(link) = handle.upgrade() else {
            eprintln!("StaticPointer::<TCell<K, MarkerLink<K>>>::upgrade failed");
            return None;
        };
        let link = link.ro(key);
        let from_pin = link.from.clone();
        let to_pin = link.to.clone();
        let from_time = link.from.upgrade().unwrap().ro(key).cached_timeline_time().value().into_f64();
        let to_time = link.to.upgrade().unwrap().ro(key).cached_timeline_time().value().into_f64();
        let from_component = marker_map.get(&link.from).cloned();
        let to_component = marker_map.get(&link.to).cloned();
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
        let len = link.len;
        Some(ComponentLinkData {
            handle,
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

pub struct ComponentLinkDataList<LinkHandle, PinHandle, ComponentHandle> {
    pub list: Vec<ComponentLinkData<LinkHandle, PinHandle, ComponentHandle>>,
}

pub type DefaultComponentLinkDataList<K, T> = ComponentLinkDataList<MarkerLinkHandle<K>, MarkerPinHandle<K>, ComponentInstanceHandle<K, T>>;

pub struct ComponentClassData<Handle> {
    pub handle: Handle,
}

impl<K, T> ComponentClassData<StaticPointer<RwLock<dyn ComponentClass<K, T>>>>
where
    K: 'static,
    T: ParameterValueType,
{
    fn new(handle: StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentClassData<StaticPointer<RwLock<dyn ComponentClass<K, T>>>> {
        ComponentClassData { handle }
    }
}

pub struct ComponentClassDataList<Handle> {
    pub list: Vec<ComponentClassData<Handle>>,
}

pub type DefaultComponentClassDataList<K, T> = ComponentClassDataList<StaticPointer<RwLock<dyn ComponentClass<K, T>>>>;

pub trait TimelineViewModel<K: 'static, T: ParameterValueType> {
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
    type ComponentLinkHandle: Clone + Eq + Hash;
    fn component_links<R>(&self, f: impl FnOnce(&ComponentLinkDataList<Self::ComponentLinkHandle, Self::MarkerPinHandle, Self::ComponentInstanceHandle>) -> R) -> R;
    fn edit_marker_link_length(&self, link: &Self::ComponentLinkHandle, value: f64);
    type ComponentClassHandle: Clone + Eq + Hash;
    fn component_classes<R>(&self, f: impl FnOnce(&ComponentClassDataList<Self::ComponentClassHandle>) -> R) -> R;
    fn add_component_instance(&self, class: Self::ComponentClassHandle);
}

pub struct TimelineViewModelImpl<K: 'static, T: ParameterValueType, GlobalUIState, MessageHandler, G, Runtime, JoinHandle> {
    key: Arc<RwLock<TCellOwner<K>>>,
    global_ui_state: Arc<GlobalUIState>,
    component_classes: Arc<ArcSwap<DefaultComponentClassDataList<K, T>>>,
    component_instances: Arc<Mutex<DefaultComponentInstanceDataList<K, T>>>,
    component_links: Arc<RwLock<DefaultComponentLinkDataList<K, T>>>,
    selected_root_component_class: Arc<RwLock<Option<RootComponentClassHandle<K, T>>>>,
    message_router: MessageRouter<MessageHandler, Runtime>,
    runtime: Runtime,
    load_timeline_task: Arc<StdMutex<Option<JoinHandleWrapper<JoinHandle>>>>,
    guard: OnceLock<G>,
}

pub enum Message<K: 'static, T: ParameterValueType> {
    GlobalUIEvent(GlobalUIEvent<K, T>),
    AddComponentInstance(StaticPointer<RwLock<dyn ComponentClass<K, T>>>),
    ClickComponentInstance(ComponentInstanceHandle<K, T>),
    DeleteComponentInstance(ComponentInstanceHandle<K, T>),
    MoveComponentInstance(ComponentInstanceHandle<K, T>, f64),
    InsertComponentInstanceTo(ComponentInstanceHandle<K, T>, usize),
    MoveMarkerPin(ComponentInstanceHandle<K, T>, MarkerPinHandle<K>, f64),
    ConnectMarkerPins(MarkerPinHandle<K>, MarkerPinHandle<K>),
    EditMarkerLinkLength(MarkerLinkHandle<K>, f64),
    EditComponentLength(MarkerTime),
    AddMarkerPin(ComponentInstanceHandle<K, T>, TimelineTime),
    DeleteMarkerPin(ComponentInstanceHandle<K, T>, MarkerPinHandle<K>),
}

impl<K, T> Clone for Message<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        match self {
            Message::GlobalUIEvent(value) => Message::GlobalUIEvent(value.clone()),
            Message::AddComponentInstance(value) => Message::AddComponentInstance(value.clone()),
            Message::ClickComponentInstance(value) => Message::ClickComponentInstance(value.clone()),
            Message::DeleteComponentInstance(value) => Message::DeleteComponentInstance(value.clone()),
            &Message::MoveComponentInstance(ref value, to) => Message::MoveComponentInstance(value.clone(), to),
            &Message::InsertComponentInstanceTo(ref instance, index) => Message::InsertComponentInstanceTo(instance.clone(), index),
            &Message::MoveMarkerPin(ref instance, ref pin, to) => Message::MoveMarkerPin(instance.clone(), pin.clone(), to),
            Message::ConnectMarkerPins(from, to) => Message::ConnectMarkerPins(from.clone(), to.clone()),
            &Message::EditMarkerLinkLength(ref value, length) => Message::EditMarkerLinkLength(value.clone(), length),
            &Message::EditComponentLength(value) => Message::EditComponentLength(value),
            &Message::AddMarkerPin(ref instance, at) => Message::AddMarkerPin(instance.clone(), at),
            Message::DeleteMarkerPin(instance, pin) => Message::DeleteMarkerPin(instance.clone(), pin.clone()),
        }
    }
}

impl<K, T> PartialEq for Message<K, T>
where
    K: 'static,
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
            _ => unreachable!(),
        }
    }
}

impl<K, T> Eq for Message<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
}

impl<K, T, S, M, G, Runtime> GlobalUIEventHandler<K, T> for TimelineViewModelImpl<K, T, S, M, G, Runtime, Runtime::JoinHandle>
where
    K: Send + Sync + 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    M: MessageHandler<Message<K, T>, Runtime>,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn handle(&self, event: GlobalUIEvent<K, T>) {
        self.message_router.handle(Message::GlobalUIEvent(event));
    }
}

impl<K, T, S, M, G, Runtime> EditEventListener<K, T> for TimelineViewModelImpl<K, T, S, M, G, Runtime, Runtime::JoinHandle>
where
    K: Send + Sync + 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    M: MessageHandler<Message<K, T>, Runtime> + Send + Sync,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn on_edit(&self, _: &RootComponentClassHandle<K, T>, _: RootComponentEditEvent<K, T>) {
        use_arc!(key = self.key, component_instances = self.component_instances, component_links = self.component_links, selected_root_component_class = self.selected_root_component_class);
        let mut task = self.load_timeline_task.lock().unwrap();
        let future = TimelineViewModelImpl::load_timeline_by_current_root_component_class(key, component_instances, component_links, selected_root_component_class);
        if let Some(handle) = task.take() {
            handle.abort();
            *task = Some(self.runtime.spawn(handle.then(|_| future)));
        } else {
            *task = Some(self.runtime.spawn(future));
        }
    }

    fn on_edit_instance(&self, _: &RootComponentClassHandle<K, T>, _: &ComponentInstanceHandle<K, T>, _: InstanceEditEvent<K, T>) {
        use_arc!(key = self.key, component_instances = self.component_instances, component_links = self.component_links, selected_root_component_class = self.selected_root_component_class);
        let mut task = self.load_timeline_task.lock().unwrap();
        let future = TimelineViewModelImpl::load_timeline_by_current_root_component_class(key, component_instances, component_links, selected_root_component_class);
        if let Some(handle) = task.take() {
            handle.abort();
            *task = Some(self.runtime.spawn(handle.then(|_| future)));
        } else {
            *task = Some(self.runtime.spawn(future));
        }
    }
}

impl<K: Send + Sync + 'static, T: ParameterValueType> TimelineViewModelImpl<K, T, (), (), (), (), ()> {
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<K, T>, Edit: EditFunnel<K, T> + 'static, P: ViewModelParams<K, T>>(
        global_ui_state: &Arc<S>,
        edit: &Arc<Edit>,
        params: &P,
    ) -> Arc<TimelineViewModelImpl<K, T, S, impl MessageHandler<Message<K, T>, P::AsyncRuntime>, <P::SubscribeEditEvent as SubscribeEditEventUsecase<K, T>>::EditEventListenerGuard, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>> {
        let component_classes = Arc::new(ArcSwap::new(Arc::new(ComponentClassDataList { list: Vec::new() })));
        let selected_components = Arc::new(RwLock::new(HashSet::new()));
        let component_links = Arc::new(RwLock::new(ComponentLinkDataList { list: Vec::new() }));
        let component_instances = Arc::new(Mutex::new(ComponentInstanceDataList { list: Vec::new() }));
        let selected_root_component_class = Arc::new(RwLock::new(None));
        let load_timeline_task = Arc::new(StdMutex::new(None::<JoinHandleWrapper<<P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>>));
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler
                    .filter_map(|message| if let Message::GlobalUIEvent(value) = message { Some(value) } else { None })
                    .multiple()
                    .handle(|handler| {
                        handler.filter_map(|event| if let GlobalUIEvent::SelectRootComponentClass(value) = event { Some(value) } else { None }).handle({
                            let runtime = params.runtime().clone();
                            use_arc!(key = params.key(), selected_root_component_class, component_instances, component_links, load_timeline_task);
                            move |root_component_class| {
                                use_arc!(key, selected_root_component_class, component_instances, component_links);
                                let mut task = load_timeline_task.lock().unwrap();
                                let future = Self::load_timeline_by_new_root_component_class(key, root_component_class, component_instances, component_links, selected_root_component_class);
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
                        use_arc!(selected_root_component_class, edit);
                        move |message| {
                            use_arc!(selected_root_component_class, edit);
                            async move {
                                let selected_root_component_class = selected_root_component_class.read().await;
                                let Some(target) = selected_root_component_class.clone() else {
                                    return;
                                };
                                drop(selected_root_component_class);
                                let command = match message {
                                    Message::AddComponentInstance(pointer) => {
                                        let class = pointer.upgrade().unwrap();
                                        let instance = class.read().await.instantiate(&pointer).await;
                                        let instance = StaticPointerOwned::new(TCell::new(instance));
                                        RootComponentEditCommand::AddComponentInstance(instance)
                                    }
                                    Message::DeleteComponentInstance(handle) => RootComponentEditCommand::DeleteComponentInstance(handle),
                                    Message::EditMarkerLinkLength(target, len) => RootComponentEditCommand::EditMarkerLinkLength(target, TimelineTime::new(MixedFraction::from_f64(len))),
                                    Message::EditComponentLength(len) => RootComponentEditCommand::EditComponentLength(len),
                                    Message::InsertComponentInstanceTo(handle, index) => RootComponentEditCommand::InsertComponentInstanceTo(handle, index),
                                    Message::ConnectMarkerPins(from, to) => RootComponentEditCommand::ConnectMarkerPins(from, to),
                                    _ => unreachable!(),
                                };
                                edit.edit(&target, command);
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
                            let (mut selected_components, mut component_instances) = tokio::join!(selected_components.write(), component_instances.lock());
                            component_instances.list.iter_mut().for_each(|ComponentInstanceData { handle, selected, .. }| *selected = *handle == target);
                            selected_components.clear();
                            selected_components.insert(target);
                        }
                    }
                })
            })
            .handle(|handler| {
                handler
                    .filter(|message| matches!(message, Message::MoveComponentInstance(_, _) | Message::MoveMarkerPin(_, _, _) | Message::AddMarkerPin(_, _) | Message::DeleteMarkerPin(_, _)))
                    .handle_async({
                        use_arc!(selected_root_component_class, edit);
                        move |message| {
                            use_arc!(selected_root_component_class, edit);
                            async move {
                                let selected_root_component_class = selected_root_component_class.read().await;
                                let Some(target_root) = selected_root_component_class.clone() else {
                                    return;
                                };
                                drop(selected_root_component_class);
                                let (target, command) = match message {
                                    Message::MoveComponentInstance(target, to) => (target, InstanceEditCommand::MoveComponentInstance(TimelineTime::new(MixedFraction::from_f64(to)))),
                                    Message::MoveMarkerPin(target, pin, to) => (target, InstanceEditCommand::MoveMarkerPin(pin, TimelineTime::new(MixedFraction::from_f64(to)))),
                                    Message::AddMarkerPin(target, at) => (target, InstanceEditCommand::AddMarkerPin(at)),
                                    Message::DeleteMarkerPin(target, pin) => (target, InstanceEditCommand::DeleteMarkerPin(pin)),
                                    _ => unreachable!(),
                                };
                                edit.edit_instance(&target_root, &target, command);
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
                    list: available_component_classes.iter().cloned().map(ComponentClassData::new).collect(),
                }));
            }
        });
        let arc = Arc::new(TimelineViewModelImpl {
            key: Arc::clone(params.key()),
            global_ui_state: Arc::clone(global_ui_state),
            component_classes,
            component_instances,
            component_links,
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
        key: Arc<RwLock<TCellOwner<K>>>,
        root_component_class: Option<RootComponentClassHandle<K, T>>,
        component_instances: Arc<Mutex<DefaultComponentInstanceDataList<K, T>>>,
        component_links: Arc<RwLock<DefaultComponentLinkDataList<K, T>>>,
        selected_root_component_class: Arc<RwLock<Option<RootComponentClassHandle<K, T>>>>,
    ) {
        let (mut selected_root_component_class, mut component_instances, mut component_links) = tokio::join!(selected_root_component_class.write(), component_instances.lock(), component_links.write());
        *selected_root_component_class = root_component_class.clone();
        Self::load_timeline_inner(key, root_component_class.as_ref(), &mut component_instances, &mut component_links).await;
    }

    async fn load_timeline_by_current_root_component_class(
        key: Arc<RwLock<TCellOwner<K>>>,
        component_instances: Arc<Mutex<DefaultComponentInstanceDataList<K, T>>>,
        component_links: Arc<RwLock<DefaultComponentLinkDataList<K, T>>>,
        selected_root_component_class: Arc<RwLock<Option<RootComponentClassHandle<K, T>>>>,
    ) {
        let (selected_root_component_class, mut component_instances, mut component_links) = tokio::join!(selected_root_component_class.read(), component_instances.lock(), component_links.write());
        Self::load_timeline_inner(key, selected_root_component_class.as_ref(), &mut component_instances, &mut component_links).await;
    }

    async fn load_timeline_inner(key: Arc<RwLock<TCellOwner<K>>>, root_component_class: Option<&RootComponentClassHandle<K, T>>, component_instances: &mut DefaultComponentInstanceDataList<K, T>, component_links: &mut DefaultComponentLinkDataList<K, T>) {
        component_instances.list.clear();
        component_links.list.clear();
        let Some(root_component_class) = root_component_class else {
            return;
        };
        let Some(root_component_class) = root_component_class.upgrade() else {
            return;
        };
        let (root_component_class, key) = tokio::join!(root_component_class.read(), key.read());
        let mut pin_map = HashMap::new();
        component_instances
            .list
            .extend(root_component_class.components().await.iter().enumerate().filter_map(|(i, handle)| ComponentInstanceData::new(handle.as_ref().clone(), &key, i, &mut pin_map)));
        let component_map = component_instances.list.iter().cloned().map(|component| (component.handle.clone(), component)).collect();
        component_links.list.extend(root_component_class.links().await.iter().filter_map(|handle| ComponentLinkData::new(handle.as_ref().clone(), &key, &pin_map, &component_map)));
    }
}

impl<K, T, S, M, G, Runtime> TimelineViewModel<K, T> for TimelineViewModelImpl<K, T, S, M, G, Runtime, Runtime::JoinHandle>
where
    K: 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    M: MessageHandler<Message<K, T>, Runtime>,
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

    type ComponentInstanceHandle = ComponentInstanceHandle<K, T>;

    type MarkerPinHandle = MarkerPinHandle<K>;

    fn component_instances<R>(&self, f: impl FnOnce(&ComponentInstanceDataList<Self::ComponentInstanceHandle, Self::MarkerPinHandle>) -> R) -> R {
        f(&self.component_instances.blocking_lock())
    }

    fn click_component_instance(&self, handle: &Self::ComponentInstanceHandle) {
        self.message_router.handle(Message::ClickComponentInstance(handle.clone()));
    }

    fn delete_component_instance(&self, handle: &Self::ComponentInstanceHandle) {
        self.message_router.handle(Message::DeleteComponentInstance(handle.clone()));
    }

    fn move_component_instance(&self, handle: &Self::ComponentInstanceHandle, to: f64) {
        self.message_router.handle(Message::MoveComponentInstance(handle.clone(), to));
    }

    fn insert_component_instance_to(&self, handle: &Self::ComponentInstanceHandle, index: usize) {
        self.message_router.handle(Message::InsertComponentInstanceTo(handle.clone(), index));
    }

    fn move_marker_pin(&self, instance_handle: &Self::ComponentInstanceHandle, pin_handle: &Self::MarkerPinHandle, to: f64) {
        self.message_router.handle(Message::MoveMarkerPin(instance_handle.clone(), pin_handle.clone(), to));
    }

    fn connect_marker_pins(&self, from: &Self::MarkerPinHandle, to: &Self::MarkerPinHandle) {
        self.message_router.handle(Message::ConnectMarkerPins(from.clone(), to.clone()));
    }

    fn add_marker_pin(&self, instance: &Self::ComponentInstanceHandle, at: TimelineTime) {
        self.message_router.handle(Message::AddMarkerPin(instance.clone(), at));
    }

    fn delete_marker_pin(&self, instance: &Self::ComponentInstanceHandle, pin: &Self::MarkerPinHandle) {
        self.message_router.handle(Message::DeleteMarkerPin(instance.clone(), pin.clone()));
    }

    type ComponentLinkHandle = MarkerLinkHandle<K>;

    fn component_links<R>(&self, f: impl FnOnce(&ComponentLinkDataList<Self::ComponentLinkHandle, Self::MarkerPinHandle, Self::ComponentInstanceHandle>) -> R) -> R {
        f(&self.component_links.blocking_read())
    }

    fn edit_marker_link_length(&self, link: &Self::ComponentLinkHandle, value: f64) {
        self.message_router.handle(Message::EditMarkerLinkLength(link.clone(), value));
    }

    type ComponentClassHandle = StaticPointer<RwLock<dyn ComponentClass<K, T>>>;

    fn component_classes<R>(&self, f: impl FnOnce(&ComponentClassDataList<Self::ComponentClassHandle>) -> R) -> R {
        f(&self.component_classes.load())
    }

    fn add_component_instance(&self, class: Self::ComponentClassHandle) {
        self.message_router.handle(Message::AddComponentInstance(class));
    }
}
