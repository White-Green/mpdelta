use arc_swap::{ArcSwap, ArcSwapOption};
use bitflags::bitflags;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::project::{Project, RootComponentClass};
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeComponentRenderer, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase,
    UndoUsecase, WriteProjectUsecase,
};
use std::ops::Deref;
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
    message_sender: UnboundedSender<ViewModelMessage>,
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
}

#[derive(Debug)]
enum ViewModelMessage {
    NewProject,
    SelectProject(usize),
    NewRootComponentClass,
    SelectRootComponentClass(usize),
}

#[derive(Debug)]
struct ViewModelInner<T, R> {
    projects: ArcSwap<Vec<(StaticPointer<RwLock<Project<T>>>, String)>>,
    selected_project: AtomicUsize,
    root_component_classes: ArcSwap<Vec<(StaticPointer<RwLock<RootComponentClass<T>>>, String)>>,
    selected_root_component_class: AtomicUsize,
    realtime_renderer: ArcSwapOption<R>,
}

impl<T, R> ViewModelInner<T, R> {
    fn new() -> ViewModelInner<T, R> {
        ViewModelInner {
            projects: Default::default(),
            selected_project: Default::default(),
            root_component_classes: Default::default(),
            selected_root_component_class: Default::default(),
            realtime_renderer: Default::default(),
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
        const PROJECTS              = 0x1;
        const PROJECT_SELECT        = 0x2;
        const ROOT_COMPONENTS       = 0x4;
        const ROOT_COMPONENT_SELECT = 0x8;
    }
}

async fn view_model_loop<T, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(
    params: ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>,
    mut message_receiver: UnboundedReceiver<ViewModelMessage>,
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
        if update_flags.contains(DataUpdateFlags::ROOT_COMPONENT_SELECT) {}
    }
}
