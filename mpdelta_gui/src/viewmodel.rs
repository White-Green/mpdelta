use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::view_model_util::use_arc;
use crate::AudioTypePlayer;
use arc_swap::ArcSwapOption;
use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::IdGenerator;
use mpdelta_core::project::{ProjectHandle, RootComponentClassHandle};
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeRenderComponentUsecase, RedoUsecase, RenderWholeComponentUsecase, SetOwnerForRootComponentClassUsecase,
    SubscribeEditEventUsecase, UndoUsecase, WriteProjectUsecase,
};
use mpdelta_message_router::handler::{IntoAsyncFunctionHandler, IntoAsyncFunctionHandlerSingle, IntoDerefHandler, MessageHandlerBuilder};
use mpdelta_message_router::{handler, MessageHandler, MessageRouter};
use mpdelta_multimedia::{AudioCodec, CodecImplement, FileFormat, VideoCodec};
use rfd::AsyncFileDialog;
use std::borrow::Cow;
use std::hash::Hash;
use std::mem;
use std::sync::Arc;
use tokio::sync::RwLock;

pub trait ViewModelParams<T: ParameterValueType> {
    type AsyncRuntime: AsyncRuntime<()> + Clone + 'static;
    type IdGenerator: IdGenerator + 'static;
    type Edit: EditUsecase<T> + 'static;
    type SubscribeEditEvent: SubscribeEditEventUsecase<T> + 'static;
    type GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<T> + 'static;
    type GetLoadedProjects: GetLoadedProjectsUsecase<T> + 'static;
    type GetRootComponentClasses: GetRootComponentClassesUsecase<T> + 'static;
    type LoadProject: LoadProjectUsecase<T> + 'static;
    type NewProject: NewProjectUsecase<T> + 'static;
    type NewRootComponentClass: NewRootComponentClassUsecase<T> + 'static;
    type RealtimeRenderComponent: RealtimeRenderComponentUsecase<T> + 'static;
    type Redo: RedoUsecase<T> + 'static;
    type SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T> + 'static;
    type Undo: UndoUsecase<T> + 'static;
    type WriteProject: WriteProjectUsecase<T> + 'static;
    type AudioPlayer: AudioTypePlayer<T::Audio> + 'static;
    type EncoderType: Send + Sync + 'static;
    type Encode: RenderWholeComponentUsecase<T, Self::EncoderType> + 'static;

    fn runtime(&self) -> &Self::AsyncRuntime;
    fn id_generator(&self) -> &Arc<Self::IdGenerator>;
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
    fn audio_player(&self) -> &Arc<Self::AudioPlayer>;
    fn available_video_codec(&self) -> &Arc<[CodecImplement<VideoCodec, Self::EncoderType>]>;
    fn available_audio_codec(&self) -> &Arc<[CodecImplement<AudioCodec, Self::EncoderType>]>;
    fn encode(&self) -> &Arc<Self::Encode>;
}

pub struct ViewModelParamsImpl<
    Runtime,
    Id,
    Edit,
    SubscribeEditEvent,
    GetAvailableComponentClasses,
    GetLoadedProjects,
    GetRootComponentClasses,
    LoadProject,
    NewProject,
    NewRootComponentClass,
    RealtimeRenderComponent,
    Redo,
    SetOwnerForRootComponentClass,
    Undo,
    WriteProject,
    AudioPlayer,
    EncoderType,
    Encode,
> {
    pub runtime: Runtime,
    pub id: Arc<Id>,
    pub edit: Arc<Edit>,
    pub subscribe_edit_event: Arc<SubscribeEditEvent>,
    pub get_available_component_classes: Arc<GetAvailableComponentClasses>,
    pub get_loaded_projects: Arc<GetLoadedProjects>,
    pub get_root_component_classes: Arc<GetRootComponentClasses>,
    pub load_project: Arc<LoadProject>,
    pub new_project: Arc<NewProject>,
    pub new_root_component_class: Arc<NewRootComponentClass>,
    pub realtime_render_component: Arc<RealtimeRenderComponent>,
    pub redo: Arc<Redo>,
    pub set_owner_for_root_component_class: Arc<SetOwnerForRootComponentClass>,
    pub undo: Arc<Undo>,
    pub write_project: Arc<WriteProject>,
    pub audio_player: Arc<AudioPlayer>,
    pub available_video_codec: Arc<[CodecImplement<VideoCodec, EncoderType>]>,
    pub available_audio_codec: Arc<[CodecImplement<AudioCodec, EncoderType>]>,
    pub encode: Arc<Encode>,
}

impl<Runtime, Id, Edit, SubscribeEditEvent, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject, AudioPlayer, EncoderType, Encode> Clone
    for ViewModelParamsImpl<
        Runtime,
        Id,
        Edit,
        SubscribeEditEvent,
        GetAvailableComponentClasses,
        GetLoadedProjects,
        GetRootComponentClasses,
        LoadProject,
        NewProject,
        NewRootComponentClass,
        RealtimeRenderComponent,
        Redo,
        SetOwnerForRootComponentClass,
        Undo,
        WriteProject,
        AudioPlayer,
        EncoderType,
        Encode,
    >
where
    Runtime: Clone,
{
    fn clone(&self) -> Self {
        let ViewModelParamsImpl {
            runtime,
            id,
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
            audio_player,
            available_video_codec,
            available_audio_codec,
            encode,
        } = self;
        ViewModelParamsImpl {
            runtime: runtime.clone(),
            id: Arc::clone(id),
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
            audio_player: Arc::clone(audio_player),
            available_video_codec: Arc::clone(available_video_codec),
            available_audio_codec: Arc::clone(available_audio_codec),
            encode: Arc::clone(encode),
        }
    }
}

impl<
        T: ParameterValueType,
        Runtime,
        Id,
        Edit,
        SubscribeEditEvent,
        GetAvailableComponentClasses,
        GetLoadedProjects,
        GetRootComponentClasses,
        LoadProject,
        NewProject,
        NewRootComponentClass,
        RealtimeRenderComponent,
        Redo,
        SetOwnerForRootComponentClass,
        Undo,
        WriteProject,
        AudioPlayer,
        EncoderType,
        Encode,
    > ViewModelParams<T>
    for ViewModelParamsImpl<
        Runtime,
        Id,
        Edit,
        SubscribeEditEvent,
        GetAvailableComponentClasses,
        GetLoadedProjects,
        GetRootComponentClasses,
        LoadProject,
        NewProject,
        NewRootComponentClass,
        RealtimeRenderComponent,
        Redo,
        SetOwnerForRootComponentClass,
        Undo,
        WriteProject,
        AudioPlayer,
        EncoderType,
        Encode,
    >
where
    Runtime: AsyncRuntime<()> + Clone + 'static,
    Id: IdGenerator + 'static,
    Edit: EditUsecase<T> + 'static,
    SubscribeEditEvent: SubscribeEditEventUsecase<T> + 'static,
    GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<T> + 'static,
    GetLoadedProjects: GetLoadedProjectsUsecase<T> + 'static,
    GetRootComponentClasses: GetRootComponentClassesUsecase<T> + 'static,
    LoadProject: LoadProjectUsecase<T> + 'static,
    NewProject: NewProjectUsecase<T> + 'static,
    NewRootComponentClass: NewRootComponentClassUsecase<T> + 'static,
    RealtimeRenderComponent: RealtimeRenderComponentUsecase<T> + 'static,
    RealtimeRenderComponent::Renderer: 'static,
    Redo: RedoUsecase<T> + 'static,
    SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T> + 'static,
    Undo: UndoUsecase<T> + 'static,
    WriteProject: WriteProjectUsecase<T> + 'static,
    AudioPlayer: AudioTypePlayer<T::Audio> + 'static,
    EncoderType: Send + Sync + 'static,
    Encode: RenderWholeComponentUsecase<T, EncoderType> + 'static,
{
    type AsyncRuntime = Runtime;
    type IdGenerator = Id;
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
    type AudioPlayer = AudioPlayer;
    type EncoderType = EncoderType;
    type Encode = Encode;

    fn runtime(&self) -> &Self::AsyncRuntime {
        &self.runtime
    }
    fn id_generator(&self) -> &Arc<Id> {
        &self.id
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
    fn audio_player(&self) -> &Arc<AudioPlayer> {
        &self.audio_player
    }
    fn available_video_codec(&self) -> &Arc<[CodecImplement<VideoCodec, Self::EncoderType>]> {
        &self.available_video_codec
    }
    fn available_audio_codec(&self) -> &Arc<[CodecImplement<AudioCodec, Self::EncoderType>]> {
        &self.available_audio_codec
    }
    fn encode(&self) -> &Arc<Self::Encode> {
        &self.encode
    }
}

pub struct ProjectData<Handle> {
    pub handle: Handle,
    pub name: String,
}

impl<T> ProjectData<ProjectHandle<T>>
where
    T: ParameterValueType,
{
    fn new(handle: ProjectHandle<T>) -> ProjectData<ProjectHandle<T>> {
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

impl<T> RootComponentClassData<RootComponentClassHandle<T>>
where
    T: ParameterValueType,
{
    fn new(handle: RootComponentClassHandle<T>) -> RootComponentClassData<RootComponentClassHandle<T>> {
        RootComponentClassData { handle, name: "RootComponentClass".to_string() }
    }
}

pub struct RootComponentClassDataList<Handle> {
    pub list: Vec<RootComponentClassData<Handle>>,
    pub selected: usize,
}

pub trait MainWindowViewModel<T> {
    fn new_project(&self);
    fn open_project(&self);
    fn save_project(&self);
    type ProjectHandle: Clone + Hash;
    fn projects<R>(&self, f: impl FnOnce(&ProjectDataList<Self::ProjectHandle>) -> R) -> R;
    fn select_project(&self, handle: &Self::ProjectHandle);
    fn new_root_component_class(&self);
    type RootComponentClassHandle: Clone + Hash;
    fn root_component_classes<R>(&self, f: impl FnOnce(&RootComponentClassDataList<Self::RootComponentClassHandle>) -> R) -> R;
    fn select_root_component_class(&self, handle: &Self::RootComponentClassHandle);
    fn render_frame<R>(&self, f: impl FnOnce() -> R) -> R;
    fn encode(&self);
}

pub struct MainWindowViewModelImpl<T: ParameterValueType, GlobalUIState, MessageHandler, Runtime> {
    projects: Arc<RwLock<ProjectDataList<ProjectHandle<T>>>>,
    root_component_classes: Arc<RwLock<RootComponentClassDataList<RootComponentClassHandle<T>>>>,
    global_ui_state: Arc<GlobalUIState>,
    message_router: MessageRouter<MessageHandler, Runtime>,
    selected_root_component_class: Arc<ArcSwapOption<RootComponentClassHandle<T>>>,
}

#[derive(Debug)]
pub enum Message<T: ParameterValueType> {
    NewProject,
    SelectProject(ProjectHandle<T>),
    NewRootComponentClass,
    SelectRootComponentClass(RootComponentClassHandle<T>),
    Encode,
    OpenProject,
    SaveProject,
}

impl<T> Clone for Message<T>
where
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        match self {
            Message::NewProject => Message::NewProject,
            Message::SelectProject(value) => Message::SelectProject(value.clone()),
            Message::NewRootComponentClass => Message::NewRootComponentClass,
            Message::SelectRootComponentClass(value) => Message::SelectRootComponentClass(value.clone()),
            Message::Encode => Message::Encode,
            Message::OpenProject => Message::OpenProject,
            Message::SaveProject => Message::SaveProject,
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
            (Message::NewProject, Message::NewProject) => true,
            (Message::SelectProject(a), Message::SelectProject(b)) => a == b,
            (Message::NewRootComponentClass, Message::NewRootComponentClass) => true,
            (Message::SelectRootComponentClass(a), Message::SelectRootComponentClass(b)) => a == b,
            (Message::Encode, Message::Encode) => true,
            (Message::OpenProject, Message::OpenProject) => true,
            (Message::SaveProject, Message::SaveProject) => true,
            _ => unreachable!(),
        }
    }
}

impl<T> Eq for Message<T> where T: ParameterValueType {}

impl<T: ParameterValueType, GlobalUIState, MessageHandler, Runtime> GlobalUIEventHandler<T> for MainWindowViewModelImpl<T, GlobalUIState, MessageHandler, Runtime> {
    fn handle(&self, event: GlobalUIEvent<T>) {
        if let GlobalUIEvent::SelectRootComponentClass(root_component_class) = event {
            self.selected_root_component_class.store(root_component_class.map(Arc::new));
        }
    }
}

impl<T: ParameterValueType> MainWindowViewModelImpl<T, (), (), ()>
where
    T: ParameterValueType,
{
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<T>, P: ViewModelParams<T>>(global_ui_state: &Arc<S>, params: &P) -> Arc<MainWindowViewModelImpl<T, S, impl MessageHandler<Message<T>, P::AsyncRuntime>, P::AsyncRuntime>> {
        let projects = Arc::new(RwLock::new(ProjectDataList { list: Vec::new(), selected: 0 }));
        let root_component_classes = Arc::new(RwLock::new(RootComponentClassDataList { list: Vec::new(), selected: 0 }));
        let reset_root_component_classes = {
            let root_component_classes = Arc::clone(&root_component_classes);
            move || async move {
                root_component_classes.write().await.list.clear();
            }
        };
        let selected_root_component_class = Arc::new(ArcSwapOption::<RootComponentClassHandle<T>>::empty());
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
            .handle(|handler| {
                handler.filter(|message| *message == Message::Encode).handle_async_single({
                    use_arc!(
                        selected_root_component_class,
                        id = params.id_generator(),
                        available_video_codec = params.available_video_codec(),
                        available_audio_codec = params.available_audio_codec(),
                        encode = params.encode()
                    );
                    move |_| {
                        use_arc!(selected_root_component_class, id, available_video_codec, available_audio_codec, encode);
                        async move {
                            if let Some(root_component_class) = selected_root_component_class.load().as_ref() {
                                let video_codec = &available_video_codec[0];
                                let audio_codec = &available_audio_codec[0];
                                let video_codec_handler = video_codec.handler();
                                assert!(video_codec_handler.eq(&**audio_codec.handler()));
                                let output_file = AsyncFileDialog::new().add_filter("video", &["mp4"]).save_file().await;
                                let Some(output_file) = output_file else {
                                    return;
                                };
                                let encoder = video_codec_handler.create_encoder(FileFormat::Mp4, Some((video_codec.codec(), video_codec.default_codec_options())), Some((audio_codec.codec(), audio_codec.default_codec_options())), output_file.inner());
                                let Some(root_component_class_ref) = root_component_class.upgrade() else {
                                    return;
                                };
                                let instance = root_component_class_ref.read().await.instantiate(&RootComponentClassHandle::clone(root_component_class).map(|weak| weak as _), &id).await;
                                if let Err(err) = encode.render_and_encode(Arc::new(instance), encoder).await {
                                    eprintln!("failed to encode by {err}");
                                }
                            }
                        }
                    }
                })
            })
            .handle(|handler| {
                handler
                    .filter(|message| *message == Message::OpenProject)
                    .then({
                        use_arc!(load_project = params.load_project(), get_loaded_projects = params.get_loaded_projects(), projects);
                        move |_| {
                            use_arc!(load_project, get_loaded_projects, projects);
                            async move {
                                let file = AsyncFileDialog::new().add_filter("mpdelta project file", &["mpdl"]).pick_file().await;
                                let Some(file) = file else {
                                    return;
                                };
                                let project = match load_project.load_project(file.inner()).await {
                                    Err(error) => {
                                        eprintln!("failed to load project by {error}");
                                        return;
                                    }
                                    Ok(project) => project,
                                };
                                let new_projects: Vec<_> = match get_loaded_projects.get_loaded_projects().await {
                                    Cow::Borrowed(slice) => slice.iter().cloned().map(ProjectData::new).collect(),
                                    Cow::Owned(vec) => vec.into_iter().map(ProjectData::new).collect(),
                                };
                                let selected = new_projects.iter().enumerate().find_map(|(i, p)| (p.handle == project).then_some(i)).unwrap_or_else(|| new_projects.len().saturating_sub(1));
                                *projects.write().await = ProjectDataList { list: new_projects, selected };
                            }
                        }
                    })
                    .handle_by(Arc::clone(&update_selected_project))
            })
            .handle(|handler| {
                handler.filter(|message| *message == Message::SaveProject).handle_async_single({
                    use_arc!(write_project = params.write_project(), projects);
                    move |_| {
                        use_arc!(write_project, projects);
                        async move {
                            let projects = projects.read().await;
                            if let Some(ProjectData { handle, .. }) = projects.list.get(projects.selected) {
                                let file = AsyncFileDialog::new().add_filter("mpdelta project file", &["mpdl"]).save_file().await;
                                let Some(file) = file else {
                                    return;
                                };
                                if let Err(error) = write_project.write_project(handle, file.inner()).await {
                                    eprintln!("failed to write project by {error}");
                                }
                            }
                        }
                    }
                })
            })
            .build(params.runtime().clone());
        let arc = Arc::new(MainWindowViewModelImpl {
            projects,
            root_component_classes,
            global_ui_state: Arc::clone(global_ui_state),
            message_router,
            selected_root_component_class,
        });
        global_ui_state.register_global_ui_event_handler(Arc::clone(&arc));
        arc
    }
}

impl<T, S, Handler, Runtime> MainWindowViewModel<T> for MainWindowViewModelImpl<T, S, Handler, Runtime>
where
    T: ParameterValueType,
    S: GlobalUIState<T>,
    Handler: MessageHandler<Message<T>, Runtime>,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn new_project(&self) {
        self.message_router.handle(Message::NewProject);
    }

    fn open_project(&self) {
        self.message_router.handle(Message::OpenProject);
    }

    fn save_project(&self) {
        self.message_router.handle(Message::SaveProject);
    }

    type ProjectHandle = ProjectHandle<T>;

    fn projects<R>(&self, f: impl FnOnce(&ProjectDataList<Self::ProjectHandle>) -> R) -> R {
        f(&self.projects.blocking_read())
    }

    fn select_project(&self, handle: &Self::ProjectHandle) {
        self.message_router.handle(Message::SelectProject(handle.clone()));
    }

    fn new_root_component_class(&self) {
        self.message_router.handle(Message::NewRootComponentClass);
    }

    type RootComponentClassHandle = RootComponentClassHandle<T>;

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

    fn encode(&self) {
        self.message_router.handle(Message::Encode);
    }
}
