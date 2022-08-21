use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase, UndoUsecase, WriteProjectUsecase,
};
use std::sync::Arc;
use tokio::runtime::Handle;

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

pub struct MPDeltaViewModel {}

impl MPDeltaViewModel {
    pub fn new<T, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(
        params: ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>,
    ) -> MPDeltaViewModel
    where
        T: ParameterValueType<'static>,
        Edit: EditUsecase<T>,
        GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<T>,
        GetLoadedProjects: GetLoadedProjectsUsecase<T>,
        GetRootComponentClasses: GetRootComponentClassesUsecase<T>,
        LoadProject: LoadProjectUsecase<T>,
        NewProject: NewProjectUsecase<T>,
        NewRootComponentClass: NewRootComponentClassUsecase<T>,
        RealtimeRenderComponent: RealtimeRenderComponentUsecase<T>,
        Redo: RedoUsecase<T>,
        SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T>,
        Undo: UndoUsecase<T>,
        WriteProject: WriteProjectUsecase<T>,
    {
        MPDeltaViewModel {}
    }
}
