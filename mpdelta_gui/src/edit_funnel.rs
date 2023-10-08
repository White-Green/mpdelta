use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::component::instance::ComponentInstanceHandle;
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
    pub fn new<K, T, Edit, Runtime>(handle: Runtime, edit: Arc<Edit>) -> Arc<EditFunnelImpl<Edit, Runtime>>
    where
        K: 'static,
        T: ParameterValueType,
        Edit: EditUsecase<K, T> + 'static,
        Runtime: AsyncRuntime<()>,
    {
        Arc::new(EditFunnelImpl { edit, handle })
    }
}

pub trait EditFunnel<K, T>: Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    fn edit(&self, target: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>);
    fn edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>);
}

impl<K, T, Edit, Runtime> EditFunnel<K, T> for EditFunnelImpl<Edit, Runtime>
where
    K: 'static,
    T: ParameterValueType,
    Edit: EditUsecase<K, T> + 'static,
    Runtime: AsyncRuntime<()>,
{
    fn edit(&self, target: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) {
        let edit = Arc::clone(&self.edit);
        let target = target.clone();
        self.handle.spawn(async move {
            if let Err(err) = edit.edit(&target, command).await {
                eprintln!("Failed to edit root component: {:?}", err);
            }
        });
    }

    fn edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) {
        let edit = Arc::clone(&self.edit);
        let root = root.clone();
        let target = target.clone();
        self.handle.spawn(async move {
            if let Err(err) = edit.edit_instance(&root, &target, command).await {
                eprintln!("Failed to edit instance: {:?}", err);
            }
        });
    }
}
