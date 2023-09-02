use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::message_router::handler::{IntoAsyncFunctionHandler, IntoFunctionHandler, MessageHandlerBuilder};
use crate::message_router::{handler, MessageHandler, MessageRouter};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use egui::epaint::ahash::{HashSet, HashSetExt};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::MarkerPin;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::EditEventListener;
use mpdelta_core::edit::{InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::{GetAvailableComponentClassesUsecase, SubscribeEditEventUsecase};
use qcell::{TCell, TCellOwner};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock};
use std::{future, mem};
use tokio::runtime::Handle;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct ComponentInstanceData<Handle> {
    pub handle: Handle,
    pub selected: bool,
    pub start_time: TimelineTime,
    pub end_time: TimelineTime,
    pub layer: f32,
}

impl<K: 'static, T> ComponentInstanceData<StaticPointer<TCell<K, ComponentInstance<K, T>>>> {
    fn new(handle: StaticPointer<TCell<K, ComponentInstance<K, T>>>, key: &TCellOwner<K>, i: usize, pin_map: &mut HashMap<StaticPointer<TCell<K, MarkerPin>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>>) -> Option<ComponentInstanceData<StaticPointer<TCell<K, ComponentInstance<K, T>>>>> {
        let component = handle.upgrade()?;
        let component = component.ro(key);
        let start_time = component.marker_left().upgrade()?.ro(key).cached_timeline_time();
        let end_time = component.marker_right().upgrade()?.ro(key).cached_timeline_time();
        pin_map.extend(component.markers().iter().map(StaticPointerOwned::reference).chain([component.marker_left().ptr(), component.marker_right().ptr()]).cloned().map(|marker| (marker, handle.clone())));
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

impl<K: 'static, T> ComponentLinkData<StaticPointer<TCell<K, MarkerLink<K>>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>> {
    fn new(
        handle: StaticPointer<TCell<K, MarkerLink<K>>>,
        key: &TCellOwner<K>,
        marker_map: &HashMap<StaticPointer<TCell<K, MarkerPin>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
        component_map: &HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, ComponentInstanceData<StaticPointer<TCell<K, ComponentInstance<K, T>>>>>,
    ) -> Option<ComponentLinkData<StaticPointer<TCell<K, MarkerLink<K>>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>>> {
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
    fn add_component_instance(&self);
}

pub struct TimelineViewModelImpl<K: 'static, T, GlobalUIState, MessageHandler, G> {
    key: Arc<RwLock<TCellOwner<K>>>,
    global_ui_state: Arc<GlobalUIState>,
    component_instances: Arc<RwLock<ComponentInstanceDataList<StaticPointer<TCell<K, ComponentInstance<K, T>>>>>>,
    component_links: Arc<RwLock<ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>>>>,
    selected_components: Arc<RwLock<HashSet<StaticPointer<TCell<K, ComponentInstance<K, T>>>>>>,
    selected_root_component_class: Arc<RwLock<Option<StaticPointer<RwLock<RootComponentClass<K, T>>>>>>,
    message_router: MessageRouter<MessageHandler>,
    runtime: Handle,
    load_timeline_task: Arc<Mutex<JoinHandle<()>>>,
    guard: OnceLock<G>,
}

pub enum Message<K: 'static, T> {
    GlobalUIEvent(GlobalUIEvent<K, T>),
    AddComponentInstance,
    ClickComponentInstance(StaticPointer<TCell<K, ComponentInstance<K, T>>>),
    DragComponentInstance(StaticPointer<TCell<K, ComponentInstance<K, T>>>, f32, f32),
    EditMarkerLinkLength(StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
}

impl<K: 'static, T> Clone for Message<K, T> {
    fn clone(&self) -> Self {
        match self {
            Message::GlobalUIEvent(value) => Message::GlobalUIEvent(value.clone()),
            Message::AddComponentInstance => Message::AddComponentInstance,
            Message::ClickComponentInstance(value) => Message::ClickComponentInstance(value.clone()),
            &Message::DragComponentInstance(ref value, x, y) => Message::DragComponentInstance(value.clone(), x, y),
            &Message::EditMarkerLinkLength(ref value, length) => Message::EditMarkerLinkLength(value.clone(), length),
        }
    }
}

impl<K: 'static, T> PartialEq for Message<K, T> {
    fn eq(&self, other: &Self) -> bool {
        if mem::discriminant(self) != mem::discriminant(other) {
            return false;
        }
        match (self, other) {
            (Message::GlobalUIEvent(a), Message::GlobalUIEvent(b)) => a == b,
            (Message::AddComponentInstance, Message::AddComponentInstance) => true,
            (Message::ClickComponentInstance(a), Message::ClickComponentInstance(b)) => a == b,
            (Message::DragComponentInstance(a, ax, ay), Message::DragComponentInstance(b, bx, by)) => a == b && ax == bx && ay == by,
            (Message::EditMarkerLinkLength(a, al), Message::EditMarkerLinkLength(b, bl)) => a == b && al == bl,
            _ => unreachable!(),
        }
    }
}

impl<K: 'static, T> Eq for Message<K, T> {}

impl<K, T, S, M, G> GlobalUIEventHandler<K, T> for TimelineViewModelImpl<K, T, S, M, G>
where
    K: Send + Sync + 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    M: MessageHandler<Message<K, T>>,
    G: Send + Sync + 'static,
{
    fn handle(&self, event: GlobalUIEvent<K, T>) {
        self.message_router.handle(Message::GlobalUIEvent(event));
    }
}

impl<K, T, S, M, G> EditEventListener<K, T> for TimelineViewModelImpl<K, T, S, M, G>
where
    K: Send + Sync + 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    M: MessageHandler<Message<K, T>> + Send + Sync,
    G: Send + Sync + 'static,
{
    fn on_edit(&self, _: &StaticPointer<RwLock<RootComponentClass<K, T>>>, _: RootComponentEditEvent<K, T>) {
        use_arc!(key = self.key, component_instances = self.component_instances, component_links = self.component_links, selected_root_component_class = self.selected_root_component_class);
        let mut task = self.load_timeline_task.lock().unwrap();
        task.abort();
        *task = self.runtime.spawn(TimelineViewModelImpl::load_timeline_by_current_root_component_class(key, component_instances, component_links, selected_root_component_class));
    }

    fn on_edit_instance(&self, _: &StaticPointer<RwLock<RootComponentClass<K, T>>>, _: &StaticPointer<TCell<K, ComponentInstance<K, T>>>, _: InstanceEditEvent<K, T>) {
        use_arc!(key = self.key, component_instances = self.component_instances, component_links = self.component_links, selected_root_component_class = self.selected_root_component_class);
        let mut task = self.load_timeline_task.lock().unwrap();
        task.abort();
        *task = self.runtime.spawn(TimelineViewModelImpl::load_timeline_by_current_root_component_class(key, component_instances, component_links, selected_root_component_class));
    }
}

impl<K: Send + Sync + 'static, T: ParameterValueType> TimelineViewModelImpl<K, T, (), (), ()> {
    pub fn new<S: GlobalUIState<K, T>, Edit: EditFunnel<K, T> + 'static, P: ViewModelParams<K, T>>(
        global_ui_state: &Arc<S>,
        edit: &Arc<Edit>,
        params: &P,
    ) -> Arc<TimelineViewModelImpl<K, T, S, impl MessageHandler<Message<K, T>>, <P::SubscribeEditEvent as SubscribeEditEventUsecase<K, T>>::EditEventListenerGuard>> {
        let selected_components = Arc::new(RwLock::new(HashSet::new()));
        let component_links = Arc::new(RwLock::new(ComponentLinkDataList { list: Vec::new() }));
        let component_instances = Arc::new(RwLock::new(ComponentInstanceDataList { list: Vec::new() }));
        let selected_root_component_class = Arc::new(RwLock::new(None));
        let load_timeline_task = Arc::new(Mutex::new(params.runtime().spawn(future::ready(()))));
        let message_router = MessageRouter::builder()
            .handle(
                handler::filter_map(|message| if let Message::GlobalUIEvent(value) = message { Some(value) } else { None })
                    .multiple()
                    .handle(handler::filter_map(|event| if let GlobalUIEvent::SelectRootComponentClass(value) = event { Some(value) } else { None }).handle({
                        let runtime = params.runtime().clone();
                        use_arc!(key = params.key(), selected_root_component_class, component_instances, component_links, load_timeline_task);
                        move |root_component_class| {
                            use_arc!(key, selected_root_component_class, component_instances, component_links);
                            let mut task = load_timeline_task.lock().unwrap();
                            task.abort();
                            *task = runtime.spawn(Self::load_timeline_by_new_root_component_class(key, root_component_class, component_instances, component_links, selected_root_component_class));
                        }
                    }))
                    .build(),
            )
            .handle(handler::filter(|message| *message == Message::AddComponentInstance).handle_async({
                use_arc!(selected_root_component_class, edit, get_available_component_classes = params.get_available_component_classes());
                move |_| {
                    use_arc!(selected_root_component_class, edit, get_available_component_classes);
                    async move {
                        let selected_root_component_class = selected_root_component_class.read().await;
                        let Some(target) = selected_root_component_class.clone() else {
                            return;
                        };
                        drop(selected_root_component_class);
                        let pointer = &get_available_component_classes.get_available_component_classes().await[0];
                        let class = pointer.upgrade().unwrap();
                        let instance = class.read().await.instantiate(pointer).await;
                        let instance = StaticPointerOwned::new(TCell::new(instance));
                        edit.edit(&target, RootComponentEditCommand::AddComponentInstance(instance));
                    }
                }
            }))
            .handle(handler::filter_map(|message| if let Message::ClickComponentInstance(value) = message { Some(value) } else { None }).handle_async({
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
            }))
            .handle(handler::filter_map(|message| if let Message::DragComponentInstance(value, x, y) = message { Some((value, x, y)) } else { None }).handle_async(move |_| async move {}))
            .handle(handler::filter_map(|message| if let Message::EditMarkerLinkLength(value, time) = message { Some((value, time)) } else { None }).handle_async({
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
            }))
            .build(params.runtime().clone());
        let arc = Arc::new(TimelineViewModelImpl {
            key: Arc::clone(params.key()),
            global_ui_state: Arc::clone(global_ui_state),
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
        root_component_class: Option<StaticPointer<RwLock<RootComponentClass<K, T>>>>,
        component_instances: Arc<RwLock<ComponentInstanceDataList<StaticPointer<TCell<K, ComponentInstance<K, T>>>>>>,
        component_links: Arc<RwLock<ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>>>>,
        selected_root_component_class: Arc<RwLock<Option<StaticPointer<RwLock<RootComponentClass<K, T>>>>>>,
    ) {
        let (mut selected_root_component_class, mut component_instances, mut component_links) = tokio::join!(selected_root_component_class.write(), component_instances.write(), component_links.write());
        *selected_root_component_class = root_component_class.clone();
        Self::load_timeline_inner(key, root_component_class.as_ref(), &mut component_instances, &mut component_links).await;
    }

    async fn load_timeline_by_current_root_component_class(
        key: Arc<RwLock<TCellOwner<K>>>,
        component_instances: Arc<RwLock<ComponentInstanceDataList<StaticPointer<TCell<K, ComponentInstance<K, T>>>>>>,
        component_links: Arc<RwLock<ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>>>>,
        selected_root_component_class: Arc<RwLock<Option<StaticPointer<RwLock<RootComponentClass<K, T>>>>>>,
    ) {
        let (selected_root_component_class, mut component_instances, mut component_links) = tokio::join!(selected_root_component_class.read(), component_instances.write(), component_links.write());
        Self::load_timeline_inner(key, selected_root_component_class.as_ref(), &mut component_instances, &mut component_links).await;
    }

    async fn load_timeline_inner(
        key: Arc<RwLock<TCellOwner<K>>>,
        root_component_class: Option<&StaticPointer<RwLock<RootComponentClass<K, T>>>>,
        component_instances: &mut ComponentInstanceDataList<StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
        component_links: &mut ComponentLinkDataList<StaticPointer<TCell<K, MarkerLink<K>>>, StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
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
        component_instances.list.extend(root_component_class.components().await.enumerate().filter_map(|(i, handle)| ComponentInstanceData::new(handle, &key, i, &mut pin_map)));
        let component_map = component_instances.list.iter().cloned().map(|component| (component.handle.clone(), component)).collect();
        component_links.list.extend(root_component_class.links().await.filter_map(|handle| ComponentLinkData::new(handle, &key, &pin_map, &component_map)));
    }
}

impl<K: 'static, T: ParameterValueType, S: GlobalUIState<K, T>, M: MessageHandler<Message<K, T>>, G> TimelineViewModel<K, T> for TimelineViewModelImpl<K, T, S, M, G> {
    fn seek(&self) -> usize {
        self.global_ui_state.seek()
    }

    type ComponentInstanceHandle = StaticPointer<TCell<K, ComponentInstance<K, T>>>;

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

    fn add_component_instance(&self) {
        self.message_router.handle(Message::AddComponentInstance);
    }
}
