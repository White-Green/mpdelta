use crate::global_ui_state::GlobalUIState;
use crate::message_router::handler::{IntoAsyncFunctionHandler, IntoDerefHandler, MessageHandlerBuilder};
use crate::message_router::{handler, MessageHandler, MessageRouter};
use crate::view_model_util::use_arc;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::project::{Project, RootComponentClass};
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase, SubscribeEditEventUsecase, UndoUsecase, WriteProjectUsecase,
};
use qcell::TCellOwner;
use std::borrow::Cow;
use std::hash::Hash;
use std::mem;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::RwLock;

pub trait ViewModelParams<K: 'static, T: ParameterValueType> {
    type Edit: EditUsecase<K, T> + 'static;
    type SubscribeEditEvent: SubscribeEditEventUsecase<K, T> + 'static;
    type GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<K, T> + 'static;
    type GetLoadedProjects: GetLoadedProjectsUsecase<K, T> + 'static;
    type GetRootComponentClasses: GetRootComponentClassesUsecase<K, T> + 'static;
    type LoadProject: LoadProjectUsecase<K, T> + 'static;
    type NewProject: NewProjectUsecase<K, T> + 'static;
    type NewRootComponentClass: NewRootComponentClassUsecase<K, T> + 'static;
    type RealtimeRenderComponent: RealtimeRenderComponentUsecase<K, T> + 'static;
    type Redo: RedoUsecase<K, T> + 'static;
    type SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<K, T> + 'static;
    type Undo: UndoUsecase<K, T> + 'static;
    type WriteProject: WriteProjectUsecase<K, T> + 'static;

    fn runtime(&self) -> &Handle;
    fn key(&self) -> &Arc<RwLock<TCellOwner<K>>>;
    fn edit(&self) -> &Arc<Self::Edit>;
    fn subscribe_edit_event(&self) -> &Arc<Self::SubscribeEditEvent>;
    fn get_available_component_classes(&self) -> &Arc<Self::GetAvailableComponentClasses>;
    fn get_loaded_projects(&self) -> &Arc<Self::GetLoadedProjects>;
    fn get_root_component_classes(&self) -> &Arc<Self::GetRootComponentClasses>;
    fn load_project(&self) -> &Arc<Self::LoadProject>;
    fn new_project(&self) -> &Arc<Self::NewProject>;
    fn new_root_component_class(&self) -> &Arc<Self::NewRootComponentClass>;
    fn realtime_render_component(&self) -> &Arc<Self::RealtimeRenderComponent>;
    fn redo(&self) -> &Arc<Self::Redo>;
    fn set_owner_for_root_component_class(&self) -> &Arc<Self::SetOwnerForRootComponentClass>;
    fn undo(&self) -> &Arc<Self::Undo>;
    fn write_project(&self) -> &Arc<Self::WriteProject>;
}

pub struct ViewModelParamsImpl<K: 'static, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> {
    runtime: Handle,
    edit: Arc<Edit>,
    subscribe_edit_event: Arc<SubscribeEditEvent>,
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
    key: Arc<RwLock<TCellOwner<K>>>,
}

impl<K, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
    ViewModelParamsImpl<K, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
{
    pub fn new(
        runtime: Handle,
        edit: Arc<Edit>,
        subscribe_edit_event: Arc<SubscribeEditEvent>,
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
        key: Arc<RwLock<TCellOwner<K>>>,
    ) -> ViewModelParamsImpl<K, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> {
        ViewModelParamsImpl {
            runtime,
            edit,
            subscribe_edit_event,
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
            key,
        }
    }
}

impl<K, T: ParameterValueType, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> ViewModelParams<K, T>
    for ViewModelParamsImpl<K, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
where
    Edit: EditUsecase<K, T> + 'static,
    SubscribeEditEvent: SubscribeEditEventUsecase<K, T> + 'static,
    GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<K, T> + 'static,
    GetLoadedProjects: GetLoadedProjectsUsecase<K, T> + 'static,
    GetRootComponentClasses: GetRootComponentClassesUsecase<K, T> + 'static,
    LoadProject: LoadProjectUsecase<K, T> + 'static,
    NewProject: NewProjectUsecase<K, T> + 'static,
    NewRootComponentClass: NewRootComponentClassUsecase<K, T> + 'static,
    RealtimeRenderComponent: RealtimeRenderComponentUsecase<K, T> + 'static,
    RealtimeRenderComponent::Renderer: 'static,
    Redo: RedoUsecase<K, T> + 'static,
    SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<K, T> + 'static,
    Undo: UndoUsecase<K, T> + 'static,
    WriteProject: WriteProjectUsecase<K, T> + 'static,
{
    type Edit = Edit;
    type SubscribeEditEvent = SubscribeEditEvent;
    type GetAvailableComponentClasses = GetAvailableComponentClasses;
    type GetLoadedProjects = GetLoadedProjects;
    type GetRootComponentClasses = GetRootComponentClasses;
    type LoadProject = LoadProject;
    type NewProject = NewProject;
    type NewRootComponentClass = NewRootComponentClass;
    type RealtimeRenderComponent = RealtimeRenderComponent;
    type Redo = Redo;
    type SetOwnerForRootComponentClass = SetOwnerForRootComponentClass;
    type Undo = Undo;
    type WriteProject = WriteProject;
    fn runtime(&self) -> &Handle {
        &self.runtime
    }
    fn key(&self) -> &Arc<RwLock<TCellOwner<K>>> {
        &self.key
    }
    fn edit(&self) -> &Arc<Edit> {
        &self.edit
    }
    fn subscribe_edit_event(&self) -> &Arc<SubscribeEditEvent> {
        &self.subscribe_edit_event
    }
    fn get_available_component_classes(&self) -> &Arc<GetAvailableComponentClasses> {
        &self.get_available_component_classes
    }
    fn get_loaded_projects(&self) -> &Arc<GetLoadedProjects> {
        &self.get_loaded_projects
    }
    fn get_root_component_classes(&self) -> &Arc<GetRootComponentClasses> {
        &self.get_root_component_classes
    }
    fn load_project(&self) -> &Arc<LoadProject> {
        &self.load_project
    }
    fn new_project(&self) -> &Arc<NewProject> {
        &self.new_project
    }
    fn new_root_component_class(&self) -> &Arc<NewRootComponentClass> {
        &self.new_root_component_class
    }
    fn realtime_render_component(&self) -> &Arc<RealtimeRenderComponent> {
        &self.realtime_render_component
    }
    fn redo(&self) -> &Arc<Redo> {
        &self.redo
    }
    fn set_owner_for_root_component_class(&self) -> &Arc<SetOwnerForRootComponentClass> {
        &self.set_owner_for_root_component_class
    }
    fn undo(&self) -> &Arc<Undo> {
        &self.undo
    }
    fn write_project(&self) -> &Arc<WriteProject> {
        &self.write_project
    }
}

pub struct ProjectData<Handle> {
    pub handle: Handle,
    pub name: String,
}

impl<K, T> ProjectData<StaticPointer<RwLock<Project<K, T>>>> {
    fn new(handle: StaticPointer<RwLock<Project<K, T>>>) -> ProjectData<StaticPointer<RwLock<Project<K, T>>>> {
        ProjectData { handle, name: "Project".to_string() }
    }
}

pub struct ProjectDataList<Handle> {
    pub list: Vec<ProjectData<Handle>>,
    pub selected: usize,
}

pub struct RootComponentClassData<Handle> {
    pub handle: Handle,
    pub name: String,
}

impl<K, T> RootComponentClassData<StaticPointer<RwLock<RootComponentClass<K, T>>>> {
    fn new(handle: StaticPointer<RwLock<RootComponentClass<K, T>>>) -> RootComponentClassData<StaticPointer<RwLock<RootComponentClass<K, T>>>> {
        RootComponentClassData { handle, name: "RootComponentClass".to_string() }
    }
}

pub struct RootComponentClassDataList<Handle> {
    pub list: Vec<RootComponentClassData<Handle>>,
    pub selected: usize,
}

pub trait MainWindowViewModel<K: 'static, T> {
    fn new_project(&self);
    type ProjectHandle: Clone + Hash;
    fn projects<R>(&self, f: impl FnOnce(&ProjectDataList<Self::ProjectHandle>) -> R) -> R;
    fn select_project(&self, handle: &Self::ProjectHandle);
    fn new_root_component_class(&self);
    type RootComponentClassHandle: Clone + Hash;
    fn root_component_classes<R>(&self, f: impl FnOnce(&RootComponentClassDataList<Self::RootComponentClassHandle>) -> R) -> R;
    fn select_root_component_class(&self, handle: &Self::RootComponentClassHandle);
    fn render_frame<R>(&self, f: impl FnOnce() -> R) -> R;
}

pub struct MainWindowViewModelImpl<K: 'static, T, GlobalUIState, MessageHandler> {
    projects: Arc<RwLock<ProjectDataList<StaticPointer<RwLock<Project<K, T>>>>>>,
    root_component_classes: Arc<RwLock<RootComponentClassDataList<StaticPointer<RwLock<RootComponentClass<K, T>>>>>>,
    global_ui_state: Arc<GlobalUIState>,
    message_router: MessageRouter<MessageHandler>,
}

#[derive(Debug)]
pub enum Message<K: 'static, T> {
    NewProject,
    SelectProject(StaticPointer<RwLock<Project<K, T>>>),
    NewRootComponentClass,
    SelectRootComponentClass(StaticPointer<RwLock<RootComponentClass<K, T>>>),
}

impl<K: 'static, T> Clone for Message<K, T> {
    fn clone(&self) -> Self {
        match self {
            Message::NewProject => Message::NewProject,
            Message::SelectProject(value) => Message::SelectProject(value.clone()),
            Message::NewRootComponentClass => Message::NewRootComponentClass,
            Message::SelectRootComponentClass(value) => Message::SelectRootComponentClass(value.clone()),
        }
    }
}

impl<K: 'static, T> PartialEq for Message<K, T> {
    fn eq(&self, other: &Self) -> bool {
        if mem::discriminant(self) != mem::discriminant(other) {
            return false;
        }
        match (self, other) {
            (Message::NewProject, Message::NewProject) => true,
            (Message::SelectProject(a), Message::SelectProject(b)) => a == b,
            (Message::NewRootComponentClass, Message::NewRootComponentClass) => true,
            (Message::SelectRootComponentClass(a), Message::SelectRootComponentClass(b)) => a == b,
            _ => unreachable!(),
        }
    }
}

impl<K: 'static, T> Eq for Message<K, T> {}

impl<K: 'static, T: ParameterValueType> MainWindowViewModelImpl<K, T, (), ()> {
    pub fn new<S: GlobalUIState<K, T>, P: ViewModelParams<K, T>>(global_ui_state: &Arc<S>, params: &P) -> Arc<MainWindowViewModelImpl<K, T, S, impl MessageHandler<Message<K, T>>>> {
        let projects = Arc::new(RwLock::new(ProjectDataList { list: Vec::new(), selected: 0 }));
        let root_component_classes = Arc::new(RwLock::new(RootComponentClassDataList { list: Vec::new(), selected: 0 }));
        let reset_root_component_classes = {
            let root_component_classes = Arc::clone(&root_component_classes);
            move || async move {
                root_component_classes.write().await.list.clear();
            }
        };
        let update_selected_project = Arc::new(handler::handle_async({
            use_arc!(projects, get_root_component_classes = params.get_root_component_classes(), root_component_classes, global_ui_state);
            move |_project| {
                use_arc!(projects, get_root_component_classes, root_component_classes, global_ui_state);
                async move {
                    let projects = projects.read().await;
                    if let Some(ProjectData { handle: selected_project, .. }) = projects.list.get(projects.selected) {
                        let new_root_component_classes: Vec<_> = match get_root_component_classes.get_root_component_classes(selected_project).await {
                            Cow::Borrowed(slice) => slice.iter().cloned().map(RootComponentClassData::new).collect(),
                            Cow::Owned(vec) => vec.into_iter().map(RootComponentClassData::new).collect(),
                        };
                        drop(projects);
                        if let Some(RootComponentClassData { handle, .. }) = new_root_component_classes.first() {
                            global_ui_state.select_root_component_class(handle);
                        } else {
                            global_ui_state.unselect_root_component_class();
                        }
                        *root_component_classes.write().await = RootComponentClassDataList { list: new_root_component_classes, selected: 0 };
                    }
                }
            }
        }));
        let message_router = MessageRouter::builder()
            .handle(
                handler::filter(|message| *message == Message::NewProject)
                    .then({
                        use_arc!(new_project = params.new_project(), get_loaded_projects = params.get_loaded_projects(), projects);
                        let reset_root_component_classes = reset_root_component_classes.clone();
                        move |_| {
                            use_arc!(new_project, get_loaded_projects, projects);
                            let reset_root_component_classes = reset_root_component_classes.clone();
                            async move {
                                tokio::join!(new_project.new_project(), reset_root_component_classes());
                                let new_projects: Vec<_> = match get_loaded_projects.get_loaded_projects().await {
                                    Cow::Borrowed(slice) => slice.iter().cloned().map(ProjectData::new).collect(),
                                    Cow::Owned(vec) => vec.into_iter().map(ProjectData::new).collect(),
                                };
                                let selected = new_projects.len().saturating_sub(1);
                                *projects.write().await = ProjectDataList { list: new_projects, selected };
                            }
                        }
                    })
                    .handle_by(Arc::clone(&update_selected_project)),
            )
            .handle(
                handler::filter_map(|message| if let Message::SelectProject(project) = message { Some(project) } else { None })
                    .then({
                        let reset_root_component_classes = reset_root_component_classes.clone();
                        use_arc!(projects);
                        move |project| {
                            let reset_root_component_classes = reset_root_component_classes.clone();
                            use_arc!(projects);
                            async move {
                                reset_root_component_classes().await;
                                let mut projects = projects.write().await;
                                if let Some(selected) = projects.list.iter().enumerate().find_map(|(i, ProjectData { handle, .. })| (*handle == project).then_some(i)) {
                                    projects.selected = selected;
                                }
                            }
                        }
                    })
                    .handle_by(Arc::clone(&update_selected_project)),
            )
            .handle(handler::filter(|message| *message == Message::NewRootComponentClass).handle_async({
                use_arc!(
                    new_root_component_class = params.new_root_component_class(),
                    set_owner_for_root_component_class = params.set_owner_for_root_component_class(),
                    get_root_component_classes = params.get_root_component_classes(),
                    projects,
                    root_component_classes,
                    global_ui_state
                );
                move |_| {
                    use_arc!(new_root_component_class, set_owner_for_root_component_class, get_root_component_classes, projects, root_component_classes, global_ui_state);
                    async move {
                        let new_root_component_class = new_root_component_class.new_root_component_class().await;
                        global_ui_state.select_root_component_class(&new_root_component_class);
                        let projects = projects.read().await;
                        let mut root_component_classes = root_component_classes.write().await;
                        if let Some(ProjectData { handle: project, .. }) = projects.list.get(projects.selected) {
                            set_owner_for_root_component_class.set_owner_for_root_component_class(&new_root_component_class, project).await;
                            let new_root_component_classes: Vec<_> = match get_root_component_classes.get_root_component_classes(project).await {
                                Cow::Borrowed(slice) => slice.iter().cloned().map(RootComponentClassData::new).collect(),
                                Cow::Owned(vec) => vec.into_iter().map(RootComponentClassData::new).collect(),
                            };
                            let selected = new_root_component_classes.iter().enumerate().find_map(|(i, RootComponentClassData { handle, .. })| (*handle == new_root_component_class).then_some(i));
                            *root_component_classes = RootComponentClassDataList {
                                list: new_root_component_classes,
                                selected: selected.unwrap_or(0),
                            };
                        } else {
                            root_component_classes.list.push(RootComponentClassData {
                                handle: new_root_component_class,
                                name: "RootComponentClass".to_string(),
                            });
                            root_component_classes.selected = root_component_classes.list.len() - 1;
                        }
                    }
                }
            }))
            .handle(handler::filter_map(|message| if let Message::SelectRootComponentClass(root_component_class) = message { Some(root_component_class) } else { None }).handle_async({
                use_arc!(root_component_classes, global_ui_state);
                move |root_component_class| {
                    use_arc!(root_component_classes, global_ui_state);
                    async move {
                        global_ui_state.select_root_component_class(&root_component_class);
                        let mut root_component_classes = root_component_classes.write().await;
                        if let Some(selected) = root_component_classes.list.iter().enumerate().find_map(|(i, RootComponentClassData { handle, .. })| (*handle == root_component_class).then_some(i)) {
                            root_component_classes.selected = selected;
                        }
                    }
                }
            }))
            .build(params.runtime().clone());
        Arc::new(MainWindowViewModelImpl {
            projects,
            root_component_classes,
            global_ui_state: Arc::clone(global_ui_state),
            message_router,
        })
    }
}

impl<K, T, S, Handler> MainWindowViewModel<K, T> for MainWindowViewModelImpl<K, T, S, Handler>
where
    K: 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    Handler: MessageHandler<Message<K, T>>,
{
    fn new_project(&self) {
        self.message_router.handle(Message::NewProject);
    }

    type ProjectHandle = StaticPointer<RwLock<Project<K, T>>>;

    fn projects<R>(&self, f: impl FnOnce(&ProjectDataList<Self::ProjectHandle>) -> R) -> R {
        f(&self.projects.blocking_read())
    }

    fn select_project(&self, handle: &Self::ProjectHandle) {
        self.message_router.handle(Message::SelectProject(handle.clone()));
    }

    fn new_root_component_class(&self) {
        self.message_router.handle(Message::NewRootComponentClass);
    }

    type RootComponentClassHandle = StaticPointer<RwLock<RootComponentClass<K, T>>>;

    fn root_component_classes<R>(&self, f: impl FnOnce(&RootComponentClassDataList<Self::RootComponentClassHandle>) -> R) -> R {
        f(&self.root_component_classes.blocking_read())
    }

    fn select_root_component_class(&self, handle: &Self::RootComponentClassHandle) {
        self.message_router.handle(Message::SelectRootComponentClass(handle.clone()));
    }

    fn render_frame<R>(&self, f: impl FnOnce() -> R) -> R {
        self.global_ui_state.begin_render_frame();
        let ret = f();
        self.global_ui_state.end_render_frame();
        ret
    }
}

// pub struct MPDeltaViewModelBase<K: 'static, T, R> {
//     handle: JoinHandle<()>,
//     message_sender: UnboundedSender<ViewModelMessage<K, T>>,
//     inner: Arc<ViewModelInner<K, T, R>>,
//     playing: bool,
//     play_start: Instant,
//     seek: usize,
//     seek_base: usize,
// }
//
// impl<K, T: ParameterValueType> MPDeltaViewModelBase<K, T, ()> {
//     pub fn new<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, set_owner_for_root_component_class, Undo, WriteProject>(
//         params: ViewModelParamsImpl<K, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, set_owner_for_root_component_class, Undo, WriteProject>,
//     ) -> MPDeltaViewModelBase<K, T, RealtimeRenderComponent::Renderer>
//         where
//             Edit: EditUsecase<K, T> + Send + Sync + 'static,
//             GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<K, T> + Send + Sync + 'static,
//             GetLoadedProjects: GetLoadedProjectsUsecase<K, T> + Send + Sync + 'static,
//             GetRootComponentClasses: GetRootComponentClassesUsecase<K, T> + Send + Sync + 'static,
//             LoadProject: LoadProjectUsecase<K, T> + Send + Sync + 'static,
//             NewProject: NewProjectUsecase<K, T> + Send + Sync + 'static,
//             NewRootComponentClass: NewRootComponentClassUsecase<K, T> + Send + Sync + 'static,
//             RealtimeRenderComponent: RealtimeRenderComponentUsecase<K, T> + Send + Sync + 'static,
//             RealtimeRenderComponent::Renderer: Send + Sync + 'static,
//             Redo: RedoUsecase<K, T> + Send + Sync + 'static,
//             set_owner_for_root_component_class: SetOwnerForRootComponentClassUsecase<K, T> + Send + Sync + 'static,
//             Undo: UndoUsecase<K, T> + Send + Sync + 'static,
//             WriteProject: WriteProjectUsecase<K, T> + Send + Sync + 'static,
//     {
//         let (message_sender, message_receiver) = mpsc::unbounded_channel();
//         let runtime = params.runtime.clone();
//         let inner = Arc::new(ViewModelInner::new());
//         let handle = runtime.spawn(view_model_loop(params, message_receiver, Arc::clone(&inner)));
//         MPDeltaViewModelBase {
//             handle,
//             message_sender,
//             inner,
//             playing: false,
//             play_start: Instant::now(),
//             seek: 0,
//             seek_base: 0,
//         }
//     }
// }
//
// struct DerefMap<S, F>(S, F);
//
// impl<S, O, F: Fn(&S) -> &O> DerefMap<S, F> {
//     fn new(value: S, map: F) -> DerefMap<S, F> {
//         DerefMap(value, map)
//     }
// }
//
// impl<S, O, F: Fn(&S) -> &O> Deref for DerefMap<S, F> {
//     type Target = O;
//
//     fn deref(&self) -> &Self::Target {
//         self.1(&self.0)
//     }
// }
//
// impl<K, T: ParameterValueType, R: RealtimeComponentRenderer<T>> MPDeltaViewModelBase<K, T, R> {
//     pub fn new_project(&self) {
//         self.message_sender.send(ViewModelMessage::NewProject).unwrap();
//     }
//
//     pub fn projects(&self) -> impl Deref<Target=impl Deref<Target=impl Deref<Target=[(StaticPointer<RwLock<Project<K, T>>>, String)]>>> + '_ {
//         self.inner.projects.load()
//     }
//
//     pub fn select_project(&self, index: usize) {
//         self.message_sender.send(ViewModelMessage::SelectProject(index)).unwrap();
//     }
//
//     pub fn selected_project(&self) -> usize {
//         self.inner.selected_project.load(atomic::Ordering::Relaxed)
//     }
//
//     pub fn new_root_component_class(&self) {
//         self.message_sender.send(ViewModelMessage::NewRootComponentClass).unwrap();
//     }
//
//     pub fn root_component_classes(&self) -> impl Deref<Target=impl Deref<Target=impl Deref<Target=[(StaticPointer<RwLock<RootComponentClass<K, T>>>, String)]>>> + '_ {
//         self.inner.root_component_classes.load()
//     }
//
//     pub fn select_root_component_class(&self, index: usize) {
//         self.message_sender.send(ViewModelMessage::SelectRootComponentClass(index)).unwrap();
//     }
//
//     pub fn selected_root_component_class(&self) -> usize {
//         self.inner.selected_root_component_class.load(atomic::Ordering::Relaxed)
//     }
//
//     pub fn get_preview_image(&mut self) -> Option<T::Image> {
//         if self.playing {
//             self.seek = (self.seek_base + (self.play_start.elapsed().as_secs_f64() * 60.).floor() as usize) % 600
//         }
//         self.inner.realtime_renderer.load().as_ref().and_then(|renderer| renderer.render_frame(self.seek).ok())
//     }
//
//     pub fn component_instances(&self) -> impl Deref<Target=DashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, ComponentInstanceRect>> {
//         DerefMap::new(self.inner.timeline_item.load(), |guard| &guard.component_instances)
//     }
//
//     pub fn component_links(&self) -> impl Deref<Target=Vec<(StaticPointer<TCell<K, MarkerLink<K>>>, MarkerLink<K>, ArcSwap<String>)>> {
//         DerefMap::new(self.inner.timeline_item.load(), |guard| &guard.component_links)
//     }
//
//     pub fn marker_pins(&self) -> impl Deref<Target=DashMap<StaticPointer<TCell<K, MarkerPin>>, (Option<StaticPointer<TCell<K, ComponentInstance<K, T>>>>, f32, TimelineTime)>> {
//         DerefMap::new(self.inner.timeline_item.load(), |guard| &guard.marker_pins)
//     }
//
//     pub fn selected_component_instance(&self) -> impl Deref<Target=Option<Arc<StaticPointer<TCell<K, ComponentInstance<K, T>>>>>> {
//         self.inner.selected_component_instance.load()
//     }
//
//     pub fn click_component_instance(&self, handle: &StaticPointer<TCell<K, ComponentInstance<K, T>>>) {
//         self.message_sender.send(ViewModelMessage::ClickComponentInstance(handle.clone())).unwrap();
//     }
//
//     pub fn drag_component_instance(&self, handle: &StaticPointer<TCell<K, ComponentInstance<K, T>>>, delta: Vec2) {
//         self.message_sender.send(ViewModelMessage::DragComponentInstance(handle.clone(), delta)).unwrap();
//     }
//
//     pub fn add_component_instance(&self) {
//         self.message_sender.send(ViewModelMessage::AddComponentInstance).unwrap();
//     }
//
//     pub fn remove_marker_link(&self, link: StaticPointer<TCell<K, MarkerLink<K>>>) {
//         self.message_sender.send(ViewModelMessage::RemoveMarkerLink(link)).unwrap();
//     }
//
//     pub fn edit_marker_link_length(&self, link: StaticPointer<TCell<K, MarkerLink<K>>>, new_length: TimelineTime) {
//         self.message_sender.send(ViewModelMessage::EditMarkerLinkLength(link, new_length)).unwrap();
//     }
//
//     pub fn image_required_params(&self) -> &Mutex<Option<ImageRequiredParams<K, T>>> {
//         &self.inner.image_required_params
//     }
//
//     pub fn updated_image_required_params(&self) {
//         self.message_sender.send(ViewModelMessage::UpdatedImageRequiredParams).unwrap();
//     }
//
//     pub fn play(&self) {
//         todo!()
//     }
//
//     pub fn pause(&self) {
//         todo!()
//     }
//
//     pub fn playing(&self) -> bool {
//         todo!()
//     }
//
//     pub fn seek(&self) -> usize {
//         todo!()
//     }
//
//     pub fn set_seek(&self, seek: usize) {
//         todo!()
//     }
// }
//
// #[derive(Debug, Clone)]
// pub struct ComponentInstanceRect {
//     pub layer: f32,
//     pub time: Range<TimelineTime>,
// }
//
// enum ViewModelMessage<K: 'static, T> {
//     NewProject,
//     SelectProject(usize),
//     NewRootComponentClass,
//     SelectRootComponentClass(usize),
//     ClickComponentInstance(StaticPointer<TCell<K, ComponentInstance<K, T>>>),
//     DragComponentInstance(StaticPointer<TCell<K, ComponentInstance<K, T>>>, Vec2),
//     AddComponentInstance,
//     RemoveMarkerLink(StaticPointer<TCell<K, MarkerLink<K>>>),
//     EditMarkerLinkLength(StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
//     UpdatedImageRequiredParams,
// }
//
// impl<K, T> Debug for ViewModelMessage<K, T> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         match self {
//             ViewModelMessage::NewProject => write!(f, "NewProject"),
//             ViewModelMessage::SelectProject(v) => f.debug_tuple("SelectProject").field(v).finish(),
//             ViewModelMessage::NewRootComponentClass => write!(f, "NewRootComponentClass"),
//             ViewModelMessage::SelectRootComponentClass(v) => f.debug_tuple("SelectRotComponentClass").field(v).finish(),
//             ViewModelMessage::ClickComponentInstance(v) => f.debug_tuple("ClickComponentInstance").field(v).finish(),
//             ViewModelMessage::DragComponentInstance(v0, v1) => f.debug_tuple("DragComponentInstance").field(v0).field(v1).finish(),
//             ViewModelMessage::AddComponentInstance => write!(f, "AddComponentInstance"),
//             ViewModelMessage::RemoveMarkerLink(v) => f.debug_tuple("RemoveMarkerLink").field(v).finish(),
//             ViewModelMessage::EditMarkerLinkLength(v0, v1) => f.debug_tuple("EditMarkerLinkLength").field(v0).field(v1).finish(),
//             ViewModelMessage::UpdatedImageRequiredParams => write!(f, "UpdatedImageRequiredParams"),
//         }
//     }
// }
//
// #[derive(Debug)]
// struct TimelineItem<K: 'static, T> {
//     component_instances: DashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, ComponentInstanceRect>,
//     marker_pins: DashMap<StaticPointer<TCell<K, MarkerPin>>, (Option<StaticPointer<TCell<K, ComponentInstance<K, T>>>>, f32, TimelineTime)>,
//     component_links: Vec<(StaticPointer<TCell<K, MarkerLink<K>>>, MarkerLink<K>, ArcSwap<String>)>,
// }
//
// impl<K, T> Default for TimelineItem<K, T> {
//     fn default() -> Self {
//         TimelineItem {
//             component_instances: Default::default(),
//             marker_pins: Default::default(),
//             component_links: Default::default(),
//         }
//     }
// }
//
// struct ViewModelInner<K: 'static, T, R> {
//     projects: ArcSwap<Vec<(StaticPointer<RwLock<Project<K, T>>>, String)>>,
//     selected_project: AtomicUsize,
//     root_component_classes: ArcSwap<Vec<(StaticPointer<RwLock<RootComponentClass<K, T>>>, String)>>,
//     selected_root_component_class: AtomicUsize,
//     component_instance: ArcSwapOption<StaticPointerOwned<TCell<K, ComponentInstance<K, T>>>>,
//     realtime_renderer: ArcSwapOption<R>,
//     timeline_item: ArcSwap<TimelineItem<K, T>>,
//     selected_component_instance: ArcSwapOption<StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
//     image_required_params: Mutex<Option<ImageRequiredParams<K, T>>>,
// }
//
// impl<K, T, R: Debug> Debug for ViewModelInner<K, T, R>
//     where
//         ArcSwap<TimelineItem<K, T>>: Debug,
//         Mutex<Option<ImageRequiredParams<K, T>>>: Debug,
// {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("ViewModelInner")
//             .field("projects", &self.projects)
//             .field("selected_project", &self.selected_project)
//             .field("root_component_classes", &self.root_component_classes)
//             .field("selected_root_component_class", &self.selected_root_component_class)
//             .field("realtime_renderer", &self.realtime_renderer)
//             .field("timeline_item", &self.timeline_item)
//             .field("selected_component_instance", &self.selected_component_instance)
//             .field("image_required_params", &self.image_required_params)
//             .finish_non_exhaustive()
//     }
// }
//
// impl<K, T, R> ViewModelInner<K, T, R> {
//     fn new() -> ViewModelInner<K, T, R> {
//         ViewModelInner {
//             projects: Default::default(),
//             selected_project: Default::default(),
//             root_component_classes: Default::default(),
//             selected_root_component_class: Default::default(),
//             component_instance: Default::default(),
//             realtime_renderer: Default::default(),
//             timeline_item: Default::default(),
//             selected_component_instance: Default::default(),
//             image_required_params: Default::default(),
//         }
//     }
// }
//
// impl<K, T, R> Default for ViewModelInner<K, T, R> {
//     fn default() -> Self {
//         ViewModelInner::new()
//     }
// }
//
// bitflags! {
//     struct DataUpdateFlags: u32 {
//         const PROJECTS                  = 0x01;
//         const PROJECT_SELECT            = 0x02;
//         const ROOT_COMPONENTS           = 0x04;
//         const ROOT_COMPONENT_SELECT     = 0x08;
//         const COMPONENT_INSTANCES       = 0x10;
//         const COMPONENT_INSTANCE_SELECT = 0x20;
//     }
// }
//
// async fn view_model_loop<K, T, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, set_owner_for_root_component_class, Undo, WriteProject>(
//     params: ViewModelParamsImpl<K, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, set_owner_for_root_component_class, Undo, WriteProject>,
//     mut message_receiver: UnboundedReceiver<ViewModelMessage<K, T>>,
//     inner: Arc<ViewModelInner<K, T, RealtimeRenderComponent::Renderer>>,
// ) where
//     T: ParameterValueType,
//     Edit: EditUsecase<K, T> + Send + Sync + 'static,
//     GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<K, T> + Send + Sync + 'static,
//     GetLoadedProjects: GetLoadedProjectsUsecase<K, T> + Send + Sync + 'static,
//     GetRootComponentClasses: GetRootComponentClassesUsecase<K, T> + Send + Sync + 'static,
//     LoadProject: LoadProjectUsecase<K, T> + Send + Sync + 'static,
//     NewProject: NewProjectUsecase<K, T> + Send + Sync + 'static,
//     NewRootComponentClass: NewRootComponentClassUsecase<K, T> + Send + Sync + 'static,
//     RealtimeRenderComponent: RealtimeRenderComponentUsecase<K, T> + Send + Sync + 'static,
//     Redo: RedoUsecase<K, T> + Send + Sync + 'static,
//     set_owner_for_root_component_class: SetOwnerForRootComponentClassUsecase<K, T> + Send + Sync + 'static,
//     Undo: UndoUsecase<K, T> + Send + Sync + 'static,
//     WriteProject: WriteProjectUsecase<K, T> + Send + Sync + 'static,
// {
//     let ViewModelParamsImpl {
//         runtime: _,
//         edit,
//         get_available_component_classes,
//         get_loaded_projects,
//         get_root_component_classes,
//         load_project: _,
//         new_project,
//         new_root_component_class,
//         realtime_render_component,
//         redo: _,
//         set_owner_for_root_component_class,
//         undo: _,
//         write_project: _,
//         key,
//     } = params;
//     let ViewModelInner {
//         projects,
//         selected_project,
//         root_component_classes,
//         selected_root_component_class,
//         component_instance,
//         realtime_renderer,
//         timeline_item,
//         selected_component_instance,
//         image_required_params,
//     } = &*inner;
//     while let Some(message) = message_receiver.recv().await {
//         let mut update_flags = DataUpdateFlags::empty();
//         match message {
//             ViewModelMessage::NewProject => {
//                 new_project.new_project().await;
//                 selected_project.store(projects.load().len(), atomic::Ordering::Relaxed);
//                 update_flags |= DataUpdateFlags::PROJECT_SELECT;
//                 update_flags |= DataUpdateFlags::PROJECTS;
//             }
//             ViewModelMessage::SelectProject(i) => {
//                 selected_project.store(i, atomic::Ordering::Relaxed);
//                 update_flags |= DataUpdateFlags::PROJECT_SELECT;
//             }
//             ViewModelMessage::NewRootComponentClass => {
//                 if let Some((project, _)) = projects.load().get(selected_project.load(atomic::Ordering::Relaxed)) {
//                     let new = new_root_component_class.new_root_component_class().await;
//                     set_owner_for_root_component_class.set_owner_for_root_component_class(&new, project).await;
//                     selected_root_component_class.store(root_component_classes.load().len(), atomic::Ordering::Relaxed);
//                     update_flags |= DataUpdateFlags::ROOT_COMPONENTS;
//                     update_flags |= DataUpdateFlags::ROOT_COMPONENT_SELECT;
//                 }
//             }
//             ViewModelMessage::SelectRootComponentClass(i) => {
//                 selected_root_component_class.store(i, atomic::Ordering::Relaxed);
//                 update_flags |= DataUpdateFlags::ROOT_COMPONENT_SELECT;
//             }
//             ViewModelMessage::ClickComponentInstance(handle) => {
//                 selected_component_instance.store(Some(Arc::new(handle)));
//                 update_flags |= DataUpdateFlags::COMPONENT_INSTANCE_SELECT;
//             }
//             ViewModelMessage::DragComponentInstance(handle, _delta) => {
//                 if let Some(_rect) = timeline_item.load().component_instances.get_mut(&handle) {
//                     // let Range { start, end } = &mut rect.time;
//                     // *start = TimelineTime::new(start.value() + delta.x as f64).unwrap();
//                     // *end = TimelineTime::new(end.value() + delta.x as f64).unwrap();
//                     // //TODO: ここでコンポーネントの移動
//                     // update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
//                 }
//             }
//             ViewModelMessage::AddComponentInstance => {
//                 if let Some((target, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
//                     let pointer = &get_available_component_classes.get_available_component_classes().await[0];
//                     let class = pointer.upgrade().unwrap();
//                     let instance = class.read().await.instantiate(pointer).await;
//                     let instance = StaticPointerOwned::new(TCell::new(instance));
//                     edit.edit(target, RootComponentEditCommand::AddComponentInstance(instance)).await.unwrap();
//                     update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
//                 }
//             }
//             ViewModelMessage::RemoveMarkerLink(link) => {
//                 if let Some((target, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
//                     edit.edit(target, RootComponentEditCommand::RemoveMarkerLink(link)).await.unwrap();
//                     update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
//                 }
//             }
//             ViewModelMessage::EditMarkerLinkLength(link, new_length) => {
//                 if let Some((target, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
//                     edit.edit(target, RootComponentEditCommand::EditMarkerLinkLength(link, new_length)).await.unwrap();
//                     update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
//                 }
//             }
//             ViewModelMessage::UpdatedImageRequiredParams => {
//                 if let Some(instance) = &*selected_component_instance.load() {
//                     if let Some(instance) = instance.upgrade() {
//                         if let Some(params) = image_required_params.lock().await.clone() {
//                             let mut key = key.write().await;
//                             instance.rw(&mut key).set_image_required_params(params);
//                             update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
//                         }
//                     }
//                 }
//             }
//         }
//         if update_flags.contains(DataUpdateFlags::PROJECTS) {
//             let new_projects = get_loaded_projects.get_loaded_projects().await;
//             let new_projects = new_projects.iter().cloned().map(|ptr| (ptr, "Project".to_string())).collect();
//             projects.store(Arc::new(new_projects));
//         }
//         if update_flags.contains(DataUpdateFlags::PROJECT_SELECT) {
//             update_flags |= DataUpdateFlags::ROOT_COMPONENTS;
//             update_flags |= DataUpdateFlags::ROOT_COMPONENT_SELECT;
//         }
//         if update_flags.contains(DataUpdateFlags::ROOT_COMPONENTS) {
//             let new_root_component_classes = if let Some((project, _)) = projects.load().get(selected_project.load(atomic::Ordering::Relaxed)) {
//                 let new_root_component_classes = get_root_component_classes.get_root_component_classes(project).await;
//                 new_root_component_classes.iter().cloned().map(|ptr| (ptr, "Class".to_string())).collect()
//             } else {
//                 Vec::new()
//             };
//             root_component_classes.store(Arc::new(new_root_component_classes));
//         }
//         if update_flags.contains(DataUpdateFlags::ROOT_COMPONENT_SELECT) {
//             selected_component_instance.store(None);
//             update_flags |= DataUpdateFlags::COMPONENT_INSTANCES;
//             update_flags |= DataUpdateFlags::COMPONENT_INSTANCE_SELECT;
//         }
//         if update_flags.contains(DataUpdateFlags::COMPONENT_INSTANCES) {
//             if let Some((class, _)) = root_component_classes.load().get(selected_root_component_class.load(atomic::Ordering::Relaxed)) {
//                 if let Some(class_ref) = class.upgrade() {
//                     let class = class.clone().map(|weak| weak as _);
//                     let instance = StaticPointerOwned::new(TCell::new(class_ref.read().await.instantiate(&class).await));
//                     let instance_reference = StaticPointerOwned::reference(&instance).clone();
//                     match realtime_render_component.render_component(&instance_reference).await {
//                         Ok(renderer) => {
//                             component_instance.store(Some(Arc::new(instance)));
//                             realtime_renderer.store(Some(Arc::new(renderer)));
//                         }
//                         Err(e) => {
//                             eprintln!("failed to create renderer by {e}");
//                         }
//                     }
//                 } else {
//                     eprintln!("upgrade failed");
//                 }
//                 if let Some(root_component_class) = class.upgrade() {
//                     let class = root_component_class.read().await;
//                     let new_component_instances = DashMap::<StaticPointer<TCell<K, ComponentInstance<K, T>>>, ComponentInstanceRect>::new();
//                     let new_marker_pins = DashMap::<StaticPointer<TCell<K, MarkerPin>>, (Option<StaticPointer<TCell<K, ComponentInstance<K, T>>>>, f32, TimelineTime)>::new();
//                     new_marker_pins.insert(class.left().await, (None, 0., TimelineTime::ZERO));
//                     new_marker_pins.insert(class.right().await, (None, 0., TimelineTime::new(10.0).unwrap()));
//                     let key = key.read().await;
//                     for component in class.components().await {
//                         if let Some(component_ref) = component.upgrade() {
//                             let guard = component_ref.ro(&key);
//                             let time_left = guard.marker_left().upgrade().unwrap().ro(&key).cached_timeline_time();
//                             let time_right = guard.marker_right().upgrade().unwrap().ro(&key).cached_timeline_time();
//                             let layer = new_component_instances.len() as f32;
//                             new_marker_pins.insert(guard.marker_left().reference(), (Some(component.clone()), layer, time_left));
//                             new_marker_pins.insert(guard.marker_right().reference(), (Some(component.clone()), layer, time_right));
//                             for pin in guard.markers() {
//                                 let time = pin.ro(&key).cached_timeline_time();
//                                 new_marker_pins.insert(StaticPointerOwned::reference(pin).clone(), (Some(component.clone()), layer, time));
//                             }
//                             new_component_instances.insert(component, ComponentInstanceRect { layer, time: time_left..time_right });
//                         }
//                     }
//                     let mut new_component_links = Vec::new();
//                     for link in class.links().await {
//                         let link_inner = link.upgrade().unwrap().ro(&key).clone();
//                         let length = link_inner.len.value().to_string();
//                         new_component_links.push((link, link_inner, ArcSwap::new(Arc::new(length))));
//                     }
//                     timeline_item.store(Arc::new(TimelineItem {
//                         component_instances: new_component_instances,
//                         marker_pins: new_marker_pins,
//                         component_links: new_component_links,
//                     }))
//                 }
//             } else {
//                 realtime_renderer.store(None);
//                 timeline_item.store(Default::default());
//             }
//         }
//         if update_flags.contains(DataUpdateFlags::COMPONENT_INSTANCE_SELECT) {
//             if let Some(instance) = &*selected_component_instance.load() {
//                 if let Some(instance) = instance.upgrade() {
//                     let key = key.read().await;
//                     *image_required_params.lock().await = instance.ro(&key).image_required_params().cloned();
//                 } else {
//                     *image_required_params.lock().await = None;
//                 }
//             } else {
//                 *image_required_params.lock().await = None;
//             }
//         }
//     }
// }
