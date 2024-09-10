use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::component::instance::ComponentInstanceId;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::usecase::EditUsecase;
use std::sync::Arc;

pub struct EditFunnelImpl<Edit, Runtime> {
    edit: Arc<Edit>,
    handle: Runtime,
}

impl EditFunnelImpl<(), ()> {
    pub fn new<T, Edit, Runtime>(handle: Runtime, edit: Arc<Edit>) -> Arc<EditFunnelImpl<Edit, Runtime>>
    where
        T: ParameterValueType,
        Edit: EditUsecase<T> + 'static,
        Runtime: AsyncRuntime<()>,
    {
        Arc::new(EditFunnelImpl { edit, handle })
    }
}

pub trait EditFunnel<T>: Send + Sync
where
    T: ParameterValueType,
{
    fn edit(&self, target: &RootComponentClassHandle<T>, command: RootComponentEditCommand<T>);
    fn edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditCommand<T>);
}

impl<T, Edit, Runtime> EditFunnel<T> for EditFunnelImpl<Edit, Runtime>
where
    T: ParameterValueType,
    Edit: EditUsecase<T> + 'static,
    Runtime: AsyncRuntime<()>,
{
    fn edit(&self, target: &RootComponentClassHandle<T>, command: RootComponentEditCommand<T>) {
        let edit = Arc::clone(&self.edit);
        let target = target.clone();
        self.handle.spawn(async move {
            if let Err(err) = edit.edit(&target, command).await {
                eprintln!("Failed to edit root component: {:?}", err);
            }
        });
    }

    fn edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditCommand<T>) {
        let edit = Arc::clone(&self.edit);
        let root = root.clone();
        let target = *target;
        self.handle.spawn(async move {
            if let Err(err) = edit.edit_instance(&root, &target, command).await {
                eprintln!("Failed to edit instance: {:?}", err);
            }
        });
    }
}
