use arc_swap::{ArcSwap, ArcSwapOption};
use bitflags::bitflags;
use dashmap::DashMap;
use egui::{Pos2, Rect, Vec2};
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::edit::RootComponentEditCommand;
use mpdelta_core::project::{Project, RootComponentClass};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeComponentRenderer, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase,
    UndoUsecase, WriteProjectUsecase,
};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, Range};
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, RwLock};
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
        MPDeltaViewModel { handle, message_sender, inner }
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

    pub fn get_preview_image(&self) -> Option<T::Image> {
        self.inner.realtime_renderer.load().as_ref().map(|renderer| renderer.render_frame(0))
    }

    pub fn component_instances(&self) -> &DashMap<StaticPointer<RwLock<ComponentInstance<T>>>, ComponentInstanceRect> {
        &self.inner.component_instances
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
}

#[derive(Debug, Clone)]
pub struct ComponentInstanceRect {
    pub layer: f32,
    pub time: Range<f32>,
}

enum ViewModelMessage<T> {
    NewProject,
    SelectProject(usize),
    NewRootComponentClass,
    SelectRootComponentClass(usize),
    ClickComponentInstance(StaticPointer<RwLock<ComponentInstance<T>>>),
    DragComponentInstance(StaticPointer<RwLock<ComponentInstance<T>>>, Vec2),
    AddComponentInstance,
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
    component_instances: DashMap<StaticPointer<RwLock<ComponentInstance<T>>>, ComponentInstanceRect>,
}

impl<T, R> ViewModelInner<T, R> {
    fn new() -> ViewModelInner<T, R> {
        ViewModelInner {
            projects: Default::default(),
            selected_project: Default::default(),
            root_component_classes: Default::default(),
            selected_root_component_class: Default::default(),
            realtime_renderer: Default::default(),
            component_instances: Default::default(),
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
        const PROJECTS              = 0x01;
        const PROJECT_SELECT        = 0x02;
        const ROOT_COMPONENTS       = 0x04;
        const ROOT_COMPONENT_SELECT = 0x08;
        const COMPONENT_INSTANCES   = 0x10;
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
        component_instances,
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
                // click
            }
            ViewModelMessage::DragComponentInstance(handle, delta) => {
                if let Some(mut rect) = component_instances.get_mut(&handle) {
                    let Range { start, end } = &mut rect.time;
                    *start += delta.x;
                    *end += delta.x;
                    update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
                }
            }
            ViewModelMessage::AddComponentInstance => {
                if let Some((target, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
                    let pointer = &get_available_component_classes.get_available_component_classes().await[0];
                    let class = pointer.upgrade().unwrap();
                    let instance = class.read().await.instantiate(pointer).await;
                    let instance = StaticPointerOwned::new(RwLock::new(instance));
                    let reference = StaticPointerOwned::reference(&instance).clone();
                    edit.edit(target, RootComponentEditCommand::AddComponentInstance(instance)).await.unwrap();
                    component_instances.insert(
                        reference,
                        ComponentInstanceRect {
                            layer: component_instances.len() as f32,
                            time: 0.0..10.0,
                        },
                    );
                    update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
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
            update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
        }
        if update_flags.contains(DataUpdateFlags::COMPONENT_INSTANCES) {
            if let Some((class, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
                if let Some(class_ref) = class.upgrade() {
                    let class = class.clone().map(|weak| weak as _);
                    let instance = StaticPointerOwned::new(RwLock::new(class_ref.read().await.instantiate(&class).await));
                    let instance_reference = StaticPointerOwned::reference(&instance).clone();
                    match realtime_render_component.render_component(&instance_reference).await {
                        Ok(renderer) => {
                            realtime_renderer.store(Some(Arc::new(renderer)));
                        }
                        Err(e) => {
                            eprintln!("failed to create renderer by {e}");
                        }
                    }
                } else {
                    eprintln!("upgrade failed");
                }
            }
        }
    }
}
