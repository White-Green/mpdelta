use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::message_router::handler::{IntoAsyncFunctionHandler, IntoFunctionHandler, MessageHandlerBuilder};
use crate::message_router::{MessageHandler, MessageRouter};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use arc_swap::ArcSwap;
use egui::epaint::ahash::{HashSet, HashSetExt};
use mpdelta_async_runtime::{AsyncRuntime, JoinHandle};
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::MarkerPinHandle;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::EditEventListener;
use mpdelta_core::edit::{InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::{GetAvailableComponentClassesUsecase, SubscribeEditEventUsecase};
use qcell::{TCell, TCellOwner};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock};
use std::{future, mem};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct ComponentInstanceData<Handle> {
    pub handle: Handle,
    pub selected: bool,
    pub start_time: TimelineTime,
    pub end_time: TimelineTime,
    pub layer: f32,
}

impl<K, T> ComponentInstanceData<ComponentInstanceHandle<K, T>>
where
    K: 'static,
    T: ParameterValueType,
{
    fn new(handle: ComponentInstanceHandle<K, T>, key: &TCellOwner<K>, i: usize, pin_map: &mut HashMap<MarkerPinHandle<K>, ComponentInstanceHandle<K, T>>) -> Option<ComponentInstanceData<ComponentInstanceHandle<K, T>>> {
        let component = handle.upgrade()?;
        let component = component.ro(key);
        let start_time = component.marker_left().upgrade()?.ro(key).cached_timeline_time();
        let end_time = component.marker_right().upgrade()?.ro(key).cached_timeline_time();
        pin_map.extend(component.markers().iter().map(StaticPointerOwned::reference).chain([component.marker_left(), component.marker_right()]).cloned().map(|marker| (marker, handle.clone())));
        Some(ComponentInstanceData {
            handle,
            selected: false,
            start_time,
            end_time,
            layer: i as f32,
        })
    }
}

pub struct ComponentInstanceDataList<Handle> {
    pub list: Vec<ComponentInstanceData<Handle>>,
}

pub struct ComponentLinkData<LinkHandle, ComponentHandle> {
    pub handle: LinkHandle,
    pub len: TimelineTime,
    pub len_str: Mutex<String>,
    pub from_component: Option<ComponentHandle>,
    pub to_component: Option<ComponentHandle>,
    pub from_layer: f32,
    pub to_layer: f32,
    pub from_time: TimelineTime,
    pub to_time: TimelineTime,
}

impl<K, T> ComponentLinkData<StaticPointer<TCell<K, MarkerLink<K>>>, ComponentInstanceHandle<K, T>>
where
    K: 'static,
    T: ParameterValueType,
{
    fn new(
        handle: StaticPointer<TCell<K, MarkerLink<K>>>,
        key: &TCellOwner<K>,
        marker_map: &HashMap<MarkerPinHandle<K>, ComponentInstanceHandle<K, T>>,
        component_map: &HashMap<ComponentInstanceHandle<K, T>, ComponentInstanceData<ComponentInstanceHandle<K, T>>>,
    ) -> Option<ComponentLinkData<StaticPointer<TCell<K, MarkerLink<K>>>, ComponentInstanceHandle<K, T>>> {
        let Some(link) = handle.upgrade() else {
            eprintln!("StaticPointer::<TCell<K, MarkerLink<K>>>::upgrade failed");
            return None;
        };
        let link = link.ro(key);
        let from_time = link.from.upgrade().unwrap().ro(key).cached_timeline_time();
        let to_time = link.to.upgrade().unwrap().ro(key).cached_timeline_time();
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
            from_component,
            to_component,
            from_layer,
            to_layer,
            from_time,
            to_time,
        })
    }
}

pub struct ComponentLinkDataList<LinkHandle, ComponentHandle> {
    pub list: Vec<ComponentLinkData<LinkHandle, ComponentHandle>>,
}

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

pub trait TimelineViewModel<K: 'static, T: ParameterValueType> {
    fn seek(&self) -> usize;
    type ComponentInstanceHandle: Clone + Hash;
    fn component_instances<R>(&self, f: impl FnOnce(&ComponentInstanceDataList<Self::ComponentInstanceHandle>) -> R) -> R;
    fn click_component_instance(&self, handle: &Self::ComponentInstanceHandle);
    fn drag_component_instance(&self, handle: &Self::ComponentInstanceHandle, dx: f32, dy: f32);
    fn is_component_instance_selected(&self, handle: &Self::ComponentInstanceHandle) -> bool;
    type ComponentLinkHandle: Clone + Hash;
    fn component_links<R>(&self, f: impl FnOnce(&ComponentLinkDataList<Self::ComponentLinkHandle, Self::ComponentInstanceHandle>) -> R) -> R;
    fn edit_marker_link_length(&self, link: &Self::ComponentLinkHandle, value: TimelineTime);
    type ComponentClassHandle: Clone + Hash;
    fn component_classes<R>(&self, f: impl FnOnce(&ComponentClassDataList<Self::ComponentClassHandle>) -> R) -> R;
    fn add_component_instance(&self, class: Self::ComponentClassHandle);
}

pub struct TimelineViewModelImpl<K: 'static, T: ParameterValueType, GlobalUIState, MessageHandler, G, Runtime, JoinHandle> {
    key: Arc<RwLock<TCellOwner<K>>>,
    global_ui_state: Arc<GlobalUIState>,
    component_classes: Arc<ArcSwap<ComponentClassDataList<StaticPointer<RwLock<dyn ComponentClass<K, T>>>>>>,
    component_instances: Arc<RwLock<ComponentInstanceDataList<ComponentInstanceHandle<K, T>>>>,
    component_links: Arc<RwLock<ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, ComponentInstanceHandle<K, T>>>>,
    selected_components: Arc<RwLock<HashSet<ComponentInstanceHandle<K, T>>>>,
    selected_root_component_class: Arc<RwLock<Option<RootComponentClassHandle<K, T>>>>,
    message_router: MessageRouter<MessageHandler, Runtime>,
    runtime: Runtime,
    load_timeline_task: Arc<Mutex<JoinHandle>>,
    guard: OnceLock<G>,
}

pub enum Message<K: 'static, T: ParameterValueType> {
    GlobalUIEvent(GlobalUIEvent<K, T>),
    AddComponentInstance(StaticPointer<RwLock<dyn ComponentClass<K, T>>>),
    ClickComponentInstance(ComponentInstanceHandle<K, T>),
    DragComponentInstance(ComponentInstanceHandle<K, T>, f32, f32),
    EditMarkerLinkLength(StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
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
            &Message::DragComponentInstance(ref value, x, y) => Message::DragComponentInstance(value.clone(), x, y),
            &Message::EditMarkerLinkLength(ref value, length) => Message::EditMarkerLinkLength(value.clone(), length),
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
            (Message::DragComponentInstance(a, ax, ay), Message::DragComponentInstance(b, bx, by)) => a == b && ax == bx && ay == by,
            (Message::EditMarkerLinkLength(a, al), Message::EditMarkerLinkLength(b, bl)) => a == b && al == bl,
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
        task.abort();
        *task = self.runtime.spawn(TimelineViewModelImpl::load_timeline_by_current_root_component_class(key, component_instances, component_links, selected_root_component_class));
    }

    fn on_edit_instance(&self, _: &RootComponentClassHandle<K, T>, _: &ComponentInstanceHandle<K, T>, _: InstanceEditEvent<K, T>) {
        use_arc!(key = self.key, component_instances = self.component_instances, component_links = self.component_links, selected_root_component_class = self.selected_root_component_class);
        let mut task = self.load_timeline_task.lock().unwrap();
        task.abort();
        *task = self.runtime.spawn(TimelineViewModelImpl::load_timeline_by_current_root_component_class(key, component_instances, component_links, selected_root_component_class));
    }
}

impl<K: Send + Sync + 'static, T: ParameterValueType> TimelineViewModelImpl<K, T, (), (), (), (), ()> {
    pub fn new<S: GlobalUIState<K, T>, Edit: EditFunnel<K, T> + 'static, P: ViewModelParams<K, T>>(
        global_ui_state: &Arc<S>,
        edit: &Arc<Edit>,
        params: &P,
    ) -> Arc<TimelineViewModelImpl<K, T, S, impl MessageHandler<Message<K, T>, P::AsyncRuntime>, <P::SubscribeEditEvent as SubscribeEditEventUsecase<K, T>>::EditEventListenerGuard, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>> {
        let component_classes = Arc::new(ArcSwap::new(Arc::new(ComponentClassDataList { list: Vec::new() })));
        let selected_components = Arc::new(RwLock::new(HashSet::new()));
        let component_links = Arc::new(RwLock::new(ComponentLinkDataList { list: Vec::new() }));
        let component_instances = Arc::new(RwLock::new(ComponentInstanceDataList { list: Vec::new() }));
        let selected_root_component_class = Arc::new(RwLock::new(None));
        let load_timeline_task = Arc::new(Mutex::new(params.runtime().spawn(future::ready(()))));
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
                                task.abort();
                                *task = runtime.spawn(Self::load_timeline_by_new_root_component_class(key, root_component_class, component_instances, component_links, selected_root_component_class));
                            }
                        })
                    })
                    .build()
            })
            .handle(|handler| {
                handler.filter_map(|message| if let Message::AddComponentInstance(value) = message { Some(value) } else { None }).handle_async({
                    use_arc!(selected_root_component_class, edit);
                    move |pointer| {
                        use_arc!(selected_root_component_class, edit);
                        async move {
                            let selected_root_component_class = selected_root_component_class.read().await;
                            let Some(target) = selected_root_component_class.clone() else {
                                return;
                            };
                            drop(selected_root_component_class);
                            let class = pointer.upgrade().unwrap();
                            let instance = class.read().await.instantiate(&pointer).await;
                            let instance = StaticPointerOwned::new(TCell::new(instance));
                            edit.edit(&target, RootComponentEditCommand::AddComponentInstance(instance));
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
                            let (mut selected_components, mut component_instances) = tokio::join!(selected_components.write(), component_instances.write());
                            component_instances.list.iter_mut().for_each(|ComponentInstanceData { handle, selected, .. }| *selected = *handle == target);
                            selected_components.clear();
                            selected_components.insert(target);
                        }
                    }
                })
            })
            .handle(|handler| handler.filter_map(|message| if let Message::DragComponentInstance(value, x, y) = message { Some((value, x, y)) } else { None }).handle_async(move |_| async move {}))
            .handle(|handler| {
                handler.filter_map(|message| if let Message::EditMarkerLinkLength(value, time) = message { Some((value, time)) } else { None }).handle_async({
                    use_arc!(selected_root_component_class, edit);
                    move |(target, len)| {
                        use_arc!(selected_root_component_class, edit);
                        async move {
                            let guard = selected_root_component_class.read().await;
                            let Some(root_component_class) = guard.as_ref() else {
                                return;
                            };
                            edit.edit(root_component_class, RootComponentEditCommand::EditMarkerLinkLength(target, len));
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
            selected_components,
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
        component_instances: Arc<RwLock<ComponentInstanceDataList<ComponentInstanceHandle<K, T>>>>,
        component_links: Arc<RwLock<ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, ComponentInstanceHandle<K, T>>>>,
        selected_root_component_class: Arc<RwLock<Option<RootComponentClassHandle<K, T>>>>,
    ) {
        let (mut selected_root_component_class, mut component_instances, mut component_links) = tokio::join!(selected_root_component_class.write(), component_instances.write(), component_links.write());
        *selected_root_component_class = root_component_class.clone();
        Self::load_timeline_inner(key, root_component_class.as_ref(), &mut component_instances, &mut component_links).await;
    }

    async fn load_timeline_by_current_root_component_class(
        key: Arc<RwLock<TCellOwner<K>>>,
        component_instances: Arc<RwLock<ComponentInstanceDataList<ComponentInstanceHandle<K, T>>>>,
        component_links: Arc<RwLock<ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, ComponentInstanceHandle<K, T>>>>,
        selected_root_component_class: Arc<RwLock<Option<RootComponentClassHandle<K, T>>>>,
    ) {
        let (selected_root_component_class, mut component_instances, mut component_links) = tokio::join!(selected_root_component_class.read(), component_instances.write(), component_links.write());
        Self::load_timeline_inner(key, selected_root_component_class.as_ref(), &mut component_instances, &mut component_links).await;
    }

    async fn load_timeline_inner(
        key: Arc<RwLock<TCellOwner<K>>>,
        root_component_class: Option<&RootComponentClassHandle<K, T>>,
        component_instances: &mut ComponentInstanceDataList<ComponentInstanceHandle<K, T>>,
        component_links: &mut ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, ComponentInstanceHandle<K, T>>,
    ) {
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
    fn seek(&self) -> usize {
        self.global_ui_state.seek()
    }

    type ComponentInstanceHandle = ComponentInstanceHandle<K, T>;

    fn component_instances<R>(&self, f: impl FnOnce(&ComponentInstanceDataList<Self::ComponentInstanceHandle>) -> R) -> R {
        f(&self.component_instances.blocking_read())
    }

    fn click_component_instance(&self, handle: &Self::ComponentInstanceHandle) {
        self.message_router.handle(Message::ClickComponentInstance(handle.clone()));
    }

    fn drag_component_instance(&self, handle: &Self::ComponentInstanceHandle, dx: f32, dy: f32) {
        self.message_router.handle(Message::DragComponentInstance(handle.clone(), dx, dy));
    }

    fn is_component_instance_selected(&self, handle: &Self::ComponentInstanceHandle) -> bool {
        self.selected_components.blocking_read().contains(handle)
    }

    type ComponentLinkHandle = StaticPointer<TCell<K, MarkerLink<K>>>;

    fn component_links<R>(&self, f: impl FnOnce(&ComponentLinkDataList<Self::ComponentLinkHandle, Self::ComponentInstanceHandle>) -> R) -> R {
        f(&self.component_links.blocking_read())
    }

    fn edit_marker_link_length(&self, link: &Self::ComponentLinkHandle, value: TimelineTime) {
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
