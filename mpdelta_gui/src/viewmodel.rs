use crate::global_ui_state::GlobalUIState;
use crate::message_router::handler::{IntoAsyncFunctionHandler, IntoDerefHandler, MessageHandlerBuilder};
use crate::message_router::{handler, MessageHandler, MessageRouter};
use crate::view_model_util::use_arc;
use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::project::{ProjectHandle, RootComponentClassHandle};
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase, SubscribeEditEventUsecase,
    UndoUsecase, WriteProjectUsecase,
};
use qcell::TCellOwner;
use std::borrow::Cow;
use std::hash::Hash;
use std::mem;
use std::sync::Arc;
use tokio::sync::RwLock;
pub trait ViewModelParams<K: 'static, T: ParameterValueType> {
    type AsyncRuntime: AsyncRuntime<()> + Clone + 'static;
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

    fn runtime(&self) -> &Self::AsyncRuntime;
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

pub struct ViewModelParamsImpl<K: 'static, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> {
    runtime: Runtime,
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

impl<K, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
    ViewModelParamsImpl<K, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
{
    pub fn new(
        runtime: Runtime,
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
    ) -> ViewModelParamsImpl<K, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> {
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

impl<K, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> Clone
    for ViewModelParamsImpl<K, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
where
    Runtime: Clone,
{
    fn clone(&self) -> Self {
        let ViewModelParamsImpl {
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
        } = self;
        ViewModelParamsImpl {
            runtime: runtime.clone(),
            edit: Arc::clone(edit),
            subscribe_edit_event: Arc::clone(subscribe_edit_event),
            get_available_component_classes: Arc::clone(get_available_component_classes),
            get_loaded_projects: Arc::clone(get_loaded_projects),
            get_root_component_classes: Arc::clone(get_root_component_classes),
            load_project: Arc::clone(load_project),
            new_project: Arc::clone(new_project),
            new_root_component_class: Arc::clone(new_root_component_class),
            realtime_render_component: Arc::clone(realtime_render_component),
            redo: Arc::clone(redo),
            set_owner_for_root_component_class: Arc::clone(set_owner_for_root_component_class),
            undo: Arc::clone(undo),
            write_project: Arc::clone(write_project),
            key: Arc::clone(key),
        }
    }
}

impl<K, T: ParameterValueType, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject> ViewModelParams<K, T>
    for ViewModelParamsImpl<K, Runtime, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>
where
    Runtime: AsyncRuntime<()> + Clone + 'static,
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
    type AsyncRuntime = Runtime;
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
    fn runtime(&self) -> &Self::AsyncRuntime {
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

impl<K, T> ProjectData<ProjectHandle<K, T>>
where
    K: 'static,
    T: ParameterValueType,
{
    fn new(handle: ProjectHandle<K, T>) -> ProjectData<ProjectHandle<K, T>> {
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

impl<K, T> RootComponentClassData<RootComponentClassHandle<K, T>>
where
    K: 'static,
    T: ParameterValueType,
{
    fn new(handle: RootComponentClassHandle<K, T>) -> RootComponentClassData<RootComponentClassHandle<K, T>> {
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

pub struct MainWindowViewModelImpl<K: 'static, T: ParameterValueType, GlobalUIState, MessageHandler, Runtime> {
    projects: Arc<RwLock<ProjectDataList<ProjectHandle<K, T>>>>,
    root_component_classes: Arc<RwLock<RootComponentClassDataList<RootComponentClassHandle<K, T>>>>,
    global_ui_state: Arc<GlobalUIState>,
    message_router: MessageRouter<MessageHandler, Runtime>,
}

#[derive(Debug)]
pub enum Message<K: 'static, T: ParameterValueType> {
    NewProject,
    SelectProject(ProjectHandle<K, T>),
    NewRootComponentClass,
    SelectRootComponentClass(RootComponentClassHandle<K, T>),
}

impl<K, T> Clone for Message<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        match self {
            Message::NewProject => Message::NewProject,
            Message::SelectProject(value) => Message::SelectProject(value.clone()),
            Message::NewRootComponentClass => Message::NewRootComponentClass,
            Message::SelectRootComponentClass(value) => Message::SelectRootComponentClass(value.clone()),
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
            (Message::NewProject, Message::NewProject) => true,
            (Message::SelectProject(a), Message::SelectProject(b)) => a == b,
            (Message::NewRootComponentClass, Message::NewRootComponentClass) => true,
            (Message::SelectRootComponentClass(a), Message::SelectRootComponentClass(b)) => a == b,
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

impl<K, T: ParameterValueType> MainWindowViewModelImpl<K, T, (), (), ()>
where
    K: 'static,
    T: ParameterValueType,
{
    pub fn new<S: GlobalUIState<K, T>, P: ViewModelParams<K, T>>(global_ui_state: &Arc<S>, params: &P) -> Arc<MainWindowViewModelImpl<K, T, S, impl MessageHandler<Message<K, T>, P::AsyncRuntime>, P::AsyncRuntime>> {
        let projects = Arc::new(RwLock::new(ProjectDataList { list: Vec::new(), selected: 0 }));
        let root_component_classes = Arc::new(RwLock::new(RootComponentClassDataList { list: Vec::new(), selected: 0 }));
        let reset_root_component_classes = {
            let root_component_classes = Arc::clone(&root_component_classes);
            move || async move {
                root_component_classes.write().await.list.clear();
            }
        };
        let update_selected_project = Arc::new(handler::handle_async::<_, P::AsyncRuntime, _, _>({
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
            .handle(|handler| {
                handler
                    .filter(|message| *message == Message::NewProject)
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
                    .handle_by(Arc::clone(&update_selected_project))
            })
            .handle(|handler| {
                handler
                    .filter_map(|message| if let Message::SelectProject(project) = message { Some(project) } else { None })
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
                    .handle_by(Arc::clone(&update_selected_project))
            })
            .handle(|handler| {
                handler.filter(|message| *message == Message::NewRootComponentClass).handle_async({
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
                })
            })
            .handle(|handler| {
                handler.filter_map(|message| if let Message::SelectRootComponentClass(root_component_class) = message { Some(root_component_class) } else { None }).handle_async({
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
                })
            })
            .build(params.runtime().clone());
        Arc::new(MainWindowViewModelImpl {
            projects,
            root_component_classes,
            global_ui_state: Arc::clone(global_ui_state),
            message_router,
        })
    }
}

impl<K, T, S, Handler, Runtime> MainWindowViewModel<K, T> for MainWindowViewModelImpl<K, T, S, Handler, Runtime>
where
    K: 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    Handler: MessageHandler<Message<K, T>, Runtime>,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn new_project(&self) {
        self.message_router.handle(Message::NewProject);
    }

    type ProjectHandle = ProjectHandle<K, T>;

    fn projects<R>(&self, f: impl FnOnce(&ProjectDataList<Self::ProjectHandle>) -> R) -> R {
        f(&self.projects.blocking_read())
    }

    fn select_project(&self, handle: &Self::ProjectHandle) {
        self.message_router.handle(Message::SelectProject(handle.clone()));
    }

    fn new_root_component_class(&self) {
        self.message_router.handle(Message::NewRootComponentClass);
    }

    type RootComponentClassHandle = RootComponentClassHandle<K, T>;

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
