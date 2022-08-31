use arc_swap::{ArcSwap, ArcSwapOption};
use bitflags::bitflags;
use dashmap::DashMap;
use egui::{Pos2, Rect, Vec2};
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::MarkerPin;
use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterValueType};
use mpdelta_core::edit::RootComponentEditCommand;
use mpdelta_core::project::{Project, RootComponentClass};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeComponentRenderer, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase,
    UndoUsecase, WriteProjectUsecase,
};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, Range};
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;

pub struct ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> {
    runtime: Handle,
    edit: Arc<Edit>,
    get_available_component_classes: Arc<GetAvailableComponentClasses>,
    get_loaded_projects: Arc<GetLoadedProjects>,
    get_root_component_classes: Arc<GetRootComponentClasses>,
    load_project: Arc<LoadProject>,
    new_project: Arc<NewProject>,
    new_root_component_class: Arc<NewRootComponentClass>,
    realtime_render_component: Arc<RealtimeRenderComponent>,
    redo: Arc<Redo>,
    set_owner_for_root_component_class: Arc<SetOwnerForRootComponentClass>,
    undo: Arc<Undo>,
    write_project: Arc<WriteProject>,
}

impl<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
    ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
{
    pub fn new(
        runtime: Handle,
        edit: Arc<Edit>,
        get_available_component_classes: Arc<GetAvailableComponentClasses>,
        get_loaded_projects: Arc<GetLoadedProjects>,
        get_root_component_classes: Arc<GetRootComponentClasses>,
        load_project: Arc<LoadProject>,
        new_project: Arc<NewProject>,
        new_root_component_class: Arc<NewRootComponentClass>,
        realtime_render_component: Arc<RealtimeRenderComponent>,
        redo: Arc<Redo>,
        set_owner_for_root_component_class: Arc<SetOwnerForRootComponentClass>,
        undo: Arc<Undo>,
        write_project: Arc<WriteProject>,
    ) -> ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> {
        ViewModelParams {
            runtime,
            edit,
            get_available_component_classes,
            get_loaded_projects,
            get_root_component_classes,
            load_project,
            new_project,
            new_root_component_class,
            realtime_render_component,
            redo,
            set_owner_for_root_component_class,
            undo,
            write_project,
        }
    }
}

pub struct MPDeltaViewModel<T, R> {
    handle: JoinHandle<()>,
    message_sender: UnboundedSender<ViewModelMessage<T>>,
    inner: Arc<ViewModelInner<T, R>>,
    playing: bool,
    play_start: Instant,
    seek: usize,
    seek_base: usize,
}

impl<T: ParameterValueType<'static>> MPDeltaViewModel<T, ()> {
    pub fn new<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(
        params: ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>,
    ) -> MPDeltaViewModel<T, RealtimeRenderComponent::Renderer>
    where
        Edit: EditUsecase<T> + Send + Sync + 'static,
        GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<T> + Send + Sync + 'static,
        GetLoadedProjects: GetLoadedProjectsUsecase<T> + Send + Sync + 'static,
        GetRootComponentClasses: GetRootComponentClassesUsecase<T> + Send + Sync + 'static,
        LoadProject: LoadProjectUsecase<T> + Send + Sync + 'static,
        NewProject: NewProjectUsecase<T> + Send + Sync + 'static,
        NewRootComponentClass: NewRootComponentClassUsecase<T> + Send + Sync + 'static,
        RealtimeRenderComponent: RealtimeRenderComponentUsecase<T> + Send + Sync + 'static,
        RealtimeRenderComponent::Renderer: Send + Sync + 'static,
        Redo: RedoUsecase<T> + Send + Sync + 'static,
        SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T> + Send + Sync + 'static,
        Undo: UndoUsecase<T> + Send + Sync + 'static,
        WriteProject: WriteProjectUsecase<T> + Send + Sync + 'static,
    {
        let (message_sender, message_receiver) = mpsc::unbounded_channel();
        let runtime = params.runtime.clone();
        let inner = Arc::new(ViewModelInner::new());
        let handle = runtime.spawn(view_model_loop(params, message_receiver, Arc::clone(&inner)));
        MPDeltaViewModel {
            handle,
            message_sender,
            inner,
            playing: false,
            play_start: Instant::now(),
            seek: 0,
            seek_base: 0,
        }
    }
}

struct DerefMap<S, F>(S, F);

impl<S, O, F: Fn(&S) -> &O> DerefMap<S, F> {
    fn new(value: S, map: F) -> DerefMap<S, F> {
        DerefMap(value, map)
    }
}

impl<S, O, F: Fn(&S) -> &O> Deref for DerefMap<S, F> {
    type Target = O;

    fn deref(&self) -> &Self::Target {
        self.1(&self.0)
    }
}

impl<T: ParameterValueType<'static>, R: RealtimeComponentRenderer<T>> MPDeltaViewModel<T, R> {
    pub fn new_project(&self) {
        self.message_sender.send(ViewModelMessage::NewProject).unwrap();
    }

    pub fn projects(&self) -> impl Deref<Target = impl Deref<Target = impl Deref<Target = [(StaticPointer<RwLock<Project<T>>>, String)]>>> + '_ {
        self.inner.projects.load()
    }

    pub fn select_project(&self, index: usize) {
        self.message_sender.send(ViewModelMessage::SelectProject(index)).unwrap();
    }

    pub fn selected_project(&self) -> usize {
        self.inner.selected_project.load(atomic::Ordering::Relaxed)
    }

    pub fn new_root_component_class(&self) {
        self.message_sender.send(ViewModelMessage::NewRootComponentClass).unwrap();
    }

    pub fn root_component_classes(&self) -> impl Deref<Target = impl Deref<Target = impl Deref<Target = [(StaticPointer<RwLock<RootComponentClass<T>>>, String)]>>> + '_ {
        self.inner.root_component_classes.load()
    }

    pub fn select_root_component_class(&self, index: usize) {
        self.message_sender.send(ViewModelMessage::SelectRootComponentClass(index)).unwrap();
    }

    pub fn selected_root_component_class(&self) -> usize {
        self.inner.selected_root_component_class.load(atomic::Ordering::Relaxed)
    }

    pub fn get_preview_image(&mut self) -> Option<T::Image> {
        if self.playing {
            self.seek = (self.seek_base + (self.play_start.elapsed().as_secs_f64() * 60.).floor() as usize) % 600
        }
        self.inner.realtime_renderer.load().as_ref().map(|renderer| renderer.render_frame(self.seek))
    }

    pub fn component_instances(&self) -> impl Deref<Target = DashMap<StaticPointer<RwLock<ComponentInstance<T>>>, ComponentInstanceRect>> {
        DerefMap::new(self.inner.timeline_item.load(), |guard| &guard.component_instances)
    }

    pub fn component_links(&self) -> impl Deref<Target = Vec<(StaticPointer<RwLock<MarkerLink>>, MarkerLink, ArcSwap<String>)>> {
        DerefMap::new(self.inner.timeline_item.load(), |guard| &guard.component_links)
    }

    pub fn marker_pins(&self) -> impl Deref<Target = DashMap<StaticPointer<RwLock<MarkerPin>>, (Option<StaticPointer<RwLock<ComponentInstance<T>>>>, f32, TimelineTime)>> {
        DerefMap::new(self.inner.timeline_item.load(), |guard| &guard.marker_pins)
    }

    pub fn selected_component_instance(&self) -> impl Deref<Target = Option<Arc<StaticPointer<RwLock<ComponentInstance<T>>>>>> {
        self.inner.selected_component_instance.load()
    }

    pub fn click_component_instance(&self, handle: &StaticPointer<RwLock<ComponentInstance<T>>>) {
        self.message_sender.send(ViewModelMessage::ClickComponentInstance(handle.clone())).unwrap();
    }

    pub fn drag_component_instance(&self, handle: &StaticPointer<RwLock<ComponentInstance<T>>>, delta: Vec2) {
        self.message_sender.send(ViewModelMessage::DragComponentInstance(handle.clone(), delta)).unwrap();
    }

    pub fn add_component_instance(&self) {
        self.message_sender.send(ViewModelMessage::AddComponentInstance).unwrap();
    }

    pub fn remove_marker_link(&self, link: StaticPointer<RwLock<MarkerLink>>) {
        self.message_sender.send(ViewModelMessage::RemoveMarkerLink(link)).unwrap();
    }

    pub fn edit_marker_link_length(&self, link: StaticPointer<RwLock<MarkerLink>>, new_length: TimelineTime) {
        self.message_sender.send(ViewModelMessage::EditMarkerLinkLength(link, new_length)).unwrap();
    }

    pub fn image_required_params(&self) -> &Mutex<Option<ImageRequiredParams<T>>> {
        &self.inner.image_required_params
    }

    pub fn updated_image_required_params(&self) {
        self.message_sender.send(ViewModelMessage::UpdatedImageRequiredParams).unwrap();
    }

    pub fn play(&mut self) {
        if !self.playing {
            self.play_start = Instant::now();
            self.seek_base = self.seek;
            self.playing = true;
        }
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn seek(&mut self) -> &mut usize {
        &mut self.seek
    }
}

#[derive(Debug, Clone)]
pub struct ComponentInstanceRect {
    pub layer: f32,
    pub time: Range<TimelineTime>,
}

enum ViewModelMessage<T> {
    NewProject,
    SelectProject(usize),
    NewRootComponentClass,
    SelectRootComponentClass(usize),
    ClickComponentInstance(StaticPointer<RwLock<ComponentInstance<T>>>),
    DragComponentInstance(StaticPointer<RwLock<ComponentInstance<T>>>, Vec2),
    AddComponentInstance,
    RemoveMarkerLink(StaticPointer<RwLock<MarkerLink>>),
    EditMarkerLinkLength(StaticPointer<RwLock<MarkerLink>>, TimelineTime),
    UpdatedImageRequiredParams,
}

impl<T> Debug for ViewModelMessage<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewModelMessage::NewProject => write!(f, "NewProject"),
            ViewModelMessage::SelectProject(v) => f.debug_tuple("SelectProject").field(v).finish(),
            ViewModelMessage::NewRootComponentClass => write!(f, "NewRootComponentClass"),
            ViewModelMessage::SelectRootComponentClass(v) => f.debug_tuple("SelectRotComponentClass").field(v).finish(),
            ViewModelMessage::ClickComponentInstance(v) => f.debug_tuple("ClickComponentInstance").field(v).finish(),
            ViewModelMessage::DragComponentInstance(v0, v1) => f.debug_tuple("DragComponentInstance").field(v0).field(v1).finish(),
            ViewModelMessage::AddComponentInstance => write!(f, "AddComponentInstance"),
            ViewModelMessage::RemoveMarkerLink(v) => f.debug_tuple("RemoveMarkerLink").field(v).finish(),
            ViewModelMessage::EditMarkerLinkLength(v0, v1) => f.debug_tuple("EditMarkerLinkLength").field(v0).field(v1).finish(),
            ViewModelMessage::UpdatedImageRequiredParams => write!(f, "UpdatedImageRequiredParams"),
        }
    }
}

#[derive(Debug)]
struct TimelineItem<T> {
    component_instances: DashMap<StaticPointer<RwLock<ComponentInstance<T>>>, ComponentInstanceRect>,
    marker_pins: DashMap<StaticPointer<RwLock<MarkerPin>>, (Option<StaticPointer<RwLock<ComponentInstance<T>>>>, f32, TimelineTime)>,
    component_links: Vec<(StaticPointer<RwLock<MarkerLink>>, MarkerLink, ArcSwap<String>)>,
}

impl<T> Default for TimelineItem<T> {
    fn default() -> Self {
        TimelineItem {
            component_instances: Default::default(),
            marker_pins: Default::default(),
            component_links: Default::default(),
        }
    }
}

#[derive(Debug)]
struct ViewModelInner<T, R> {
    projects: ArcSwap<Vec<(StaticPointer<RwLock<Project<T>>>, String)>>,
    selected_project: AtomicUsize,
    root_component_classes: ArcSwap<Vec<(StaticPointer<RwLock<RootComponentClass<T>>>, String)>>,
    selected_root_component_class: AtomicUsize,
    realtime_renderer: ArcSwapOption<R>,
    timeline_item: ArcSwap<TimelineItem<T>>,
    selected_component_instance: ArcSwapOption<StaticPointer<RwLock<ComponentInstance<T>>>>,
    image_required_params: Mutex<Option<ImageRequiredParams<T>>>,
}

impl<T, R> ViewModelInner<T, R> {
    fn new() -> ViewModelInner<T, R> {
        ViewModelInner {
            projects: Default::default(),
            selected_project: Default::default(),
            root_component_classes: Default::default(),
            selected_root_component_class: Default::default(),
            realtime_renderer: Default::default(),
            timeline_item: Default::default(),
            selected_component_instance: Default::default(),
            image_required_params: Default::default(),
        }
    }
}

impl<T, R> Default for ViewModelInner<T, R> {
    fn default() -> Self {
        ViewModelInner::new()
    }
}

bitflags! {
    struct DataUpdateFlags: u32 {
        const PROJECTS                  = 0x01;
        const PROJECT_SELECT            = 0x02;
        const ROOT_COMPONENTS           = 0x04;
        const ROOT_COMPONENT_SELECT     = 0x08;
        const COMPONENT_INSTANCES       = 0x10;
        const COMPONENT_INSTANCE_SELECT = 0x20;
    }
}

async fn view_model_loop<T, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(
    params: ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>,
    mut message_receiver: UnboundedReceiver<ViewModelMessage<T>>,
    inner: Arc<ViewModelInner<T, RealtimeRenderComponent::Renderer>>,
) where
    T: ParameterValueType<'static>,
    Edit: EditUsecase<T> + Send + Sync + 'static,
    GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<T> + Send + Sync + 'static,
    GetLoadedProjects: GetLoadedProjectsUsecase<T> + Send + Sync + 'static,
    GetRootComponentClasses: GetRootComponentClassesUsecase<T> + Send + Sync + 'static,
    LoadProject: LoadProjectUsecase<T> + Send + Sync + 'static,
    NewProject: NewProjectUsecase<T> + Send + Sync + 'static,
    NewRootComponentClass: NewRootComponentClassUsecase<T> + Send + Sync + 'static,
    RealtimeRenderComponent: RealtimeRenderComponentUsecase<T> + Send + Sync + 'static,
    Redo: RedoUsecase<T> + Send + Sync + 'static,
    SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T> + Send + Sync + 'static,
    Undo: UndoUsecase<T> + Send + Sync + 'static,
    WriteProject: WriteProjectUsecase<T> + Send + Sync + 'static,
{
    let ViewModelParams {
        runtime,
        edit,
        get_available_component_classes,
        get_loaded_projects,
        get_root_component_classes,
        load_project,
        new_project,
        new_root_component_class,
        realtime_render_component,
        redo,
        set_owner_for_root_component_class,
        undo,
        write_project,
    } = params;
    let ViewModelInner {
        projects,
        selected_project,
        root_component_classes,
        selected_root_component_class,
        realtime_renderer,
        timeline_item,
        selected_component_instance,
        image_required_params,
    } = &*inner;
    while let Some(message) = message_receiver.recv().await {
        let mut update_flags = DataUpdateFlags::empty();
        match message {
            ViewModelMessage::NewProject => {
                new_project.new_project().await;
                selected_project.store(projects.load().len(), atomic::Ordering::Relaxed);
                update_flags |= DataUpdateFlags::PROJECT_SELECT;
                update_flags |= DataUpdateFlags::PROJECTS;
            }
            ViewModelMessage::SelectProject(i) => {
                selected_project.store(i, atomic::Ordering::Relaxed);
                update_flags |= DataUpdateFlags::PROJECT_SELECT;
            }
            ViewModelMessage::NewRootComponentClass => {
                if let Some((project, _)) = projects.load().get(selected_project.load(atomic::Ordering::Relaxed)) {
                    let new = new_root_component_class.new_root_component_class().await;
                    set_owner_for_root_component_class.set_owner_for_root_component_class(&new, project).await;
                    selected_root_component_class.store(root_component_classes.load().len(), atomic::Ordering::Relaxed);
                    update_flags |= DataUpdateFlags::ROOT_COMPONENTS;
                    update_flags |= DataUpdateFlags::ROOT_COMPONENT_SELECT;
                }
            }
            ViewModelMessage::SelectRootComponentClass(i) => {
                selected_root_component_class.store(i, atomic::Ordering::Relaxed);
                update_flags |= DataUpdateFlags::ROOT_COMPONENT_SELECT;
            }
            ViewModelMessage::ClickComponentInstance(handle) => {
                selected_component_instance.store(Some(Arc::new(handle)));
                update_flags |= DataUpdateFlags::COMPONENT_INSTANCE_SELECT;
            }
            ViewModelMessage::DragComponentInstance(handle, delta) => {
                if let Some(mut rect) = timeline_item.load().component_instances.get_mut(&handle) {
                    // let Range { start, end } = &mut rect.time;
                    // *start = TimelineTime::new(start.value() + delta.x as f64).unwrap();
                    // *end = TimelineTime::new(end.value() + delta.x as f64).unwrap();
                    // //TODO: ここでコンポーネントの移動
                    // update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
                }
            }
            ViewModelMessage::AddComponentInstance => {
                if let Some((target, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
                    let pointer = &get_available_component_classes.get_available_component_classes().await[0];
                    let class = pointer.upgrade().unwrap();
                    let instance = class.read().await.instantiate(pointer).await;
                    let instance = StaticPointerOwned::new(RwLock::new(instance));
                    edit.edit(target, RootComponentEditCommand::AddComponentInstance(instance)).await.unwrap();
                    update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
                }
            }
            ViewModelMessage::RemoveMarkerLink(link) => {
                if let Some((target, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
                    edit.edit(target, RootComponentEditCommand::RemoveMarkerLink(link)).await.unwrap();
                    update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
                }
            }
            ViewModelMessage::EditMarkerLinkLength(link, new_length) => {
                if let Some((target, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
                    edit.edit(target, RootComponentEditCommand::EditMarkerLinkLength(link, new_length)).await.unwrap();
                    update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
                }
            }
            ViewModelMessage::UpdatedImageRequiredParams => {
                if let Some(instance) = &*selected_component_instance.load() {
                    if let Some(instance) = instance.upgrade() {
                        if let Some(params) = image_required_params.lock().await.clone() {
                            instance.write().await.set_image_required_params(params);
                            update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
                        }
                    }
                }
            }
        }
        if update_flags.contains(DataUpdateFlags::PROJECTS) {
            let new_projects = get_loaded_projects.get_loaded_projects().await;
            let new_projects = new_projects.iter().cloned().map(|ptr| (ptr, "Project".to_string())).collect();
            projects.store(Arc::new(new_projects));
        }
        if update_flags.contains(DataUpdateFlags::PROJECT_SELECT) {
            update_flags |= DataUpdateFlags::ROOT_COMPONENTS;
            update_flags |= DataUpdateFlags::ROOT_COMPONENT_SELECT;
        }
        if update_flags.contains(DataUpdateFlags::ROOT_COMPONENTS) {
            let new_root_component_classes = if let Some((project, _)) = projects.load().get(selected_project.load(atomic::Ordering::Relaxed)) {
                let new_root_component_classes = get_root_component_classes.get_root_component_classes(project).await;
                new_root_component_classes.iter().cloned().map(|ptr| (ptr, "Class".to_string())).collect()
            } else {
                Vec::new()
            };
            root_component_classes.store(Arc::new(new_root_component_classes));
        }
        if update_flags.contains(DataUpdateFlags::ROOT_COMPONENT_SELECT) {
            selected_component_instance.store(None);
            update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
            update_flags |= DataUpdateFlags::COMPONENT_INSTANCE_SELECT;
        }
        if update_flags.contains(DataUpdateFlags::COMPONENT_INSTANCES) {
            if let Some((class, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
                if let Some(class_ref) = class.upgrade() {
                    let class = class.clone().map(|weak| weak as _);
                    let instance = StaticPointerOwned::new(RwLock::new(class_ref.read().await.instantiate(&class).await));
                    let instance_reference = StaticPointerOwned::reference(&instance).clone();
                    match realtime_render_component.render_component(&instance_reference).await {
                        Ok(renderer) => realtime_renderer.store(Some(Arc::new(renderer))),
                        Err(e) => {
                            eprintln!("failed to create renderer by {e}");
                        }
                    }
                } else {
                    eprintln!("upgrade failed");
                }
                if let Some(root_component_class) = class.upgrade() {
                    let class = root_component_class.read().await;
                    let new_component_instances = DashMap::<StaticPointer<RwLock<ComponentInstance<T>>>, ComponentInstanceRect>::new();
                    let new_marker_pins = DashMap::<StaticPointer<RwLock<MarkerPin>>, (Option<StaticPointer<RwLock<ComponentInstance<T>>>>, f32, TimelineTime)>::new();
                    new_marker_pins.insert(class.left().await, (None, 0., TimelineTime::ZERO));
                    new_marker_pins.insert(class.right().await, (None, 0., TimelineTime::new(10.0).unwrap()));
                    for component in class.components().await {
                        if let Some(component_ref) = component.upgrade() {
                            let guard = component_ref.read().await;
                            let time_left = guard.marker_left().upgrade().unwrap().read().await.cached_timeline_time();
                            let time_right = guard.marker_right().upgrade().unwrap().read().await.cached_timeline_time();
                            let layer = new_component_instances.len() as f32;
                            new_marker_pins.insert(guard.marker_left().reference(), (Some(component.clone()), layer, time_left));
                            new_marker_pins.insert(guard.marker_right().reference(), (Some(component.clone()), layer, time_right));
                            for pin in guard.markers() {
                                let time = pin.read().await.cached_timeline_time();
                                new_marker_pins.insert(StaticPointerOwned::reference(pin).clone(), (Some(component.clone()), layer, time));
                            }
                            drop(guard);
                            new_component_instances.insert(component, ComponentInstanceRect { layer: layer as f32, time: time_left..time_right });
                        }
                    }
                    let mut new_component_links = Vec::new();
                    for link in class.links().await {
                        let link_inner = link.upgrade().unwrap().read().await.clone();
                        let length = link_inner.len.value().to_string();
                        new_component_links.push((link, link_inner, ArcSwap::new(Arc::new(length))));
                    }
                    timeline_item.store(Arc::new(TimelineItem {
                        component_instances: new_component_instances,
                        marker_pins: new_marker_pins,
                        component_links: new_component_links,
                    }))
                }
            }
        }
        if update_flags.contains(DataUpdateFlags::COMPONENT_INSTANCE_SELECT) {
            if let Some(instance) = &*selected_component_instance.load() {
                if let Some(instance) = instance.upgrade() {
                    *image_required_params.lock().await = instance.read().await.image_required_params().cloned();
                } else {
                    *image_required_params.lock().await = None;
                }
            } else {
                *image_required_params.lock().await = None;
            }
        }
    }
}
