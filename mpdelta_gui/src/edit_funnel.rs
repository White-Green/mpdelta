use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::usecase::EditUsecase;
use qcell::TCell;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct EditFunnelImpl<Edit, Runtime> {
    edit: Arc<Edit>,
    handle: Runtime,
}

impl EditFunnelImpl<(), ()> {
    pub fn new<K: 'static, T, Edit: EditUsecase<K, T> + 'static, Runtime: AsyncRuntime<()>>(handle: Runtime, edit: Arc<Edit>) -> Arc<EditFunnelImpl<Edit, Runtime>> {
        Arc::new(EditFunnelImpl { edit, handle })
    }
}

pub trait EditFunnel<K: 'static, T>: Send + Sync {
    fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<K, T>>>, command: RootComponentEditCommand<K, T>);
    fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<K, T>>>, target: &StaticPointer<TCell<K, ComponentInstance<K, T>>>, command: InstanceEditCommand<K, T>);
}

impl<K: 'static, T: ParameterValueType, Edit: EditUsecase<K, T> + 'static, Runtime: AsyncRuntime<()>> EditFunnel<K, T> for EditFunnelImpl<Edit, Runtime> {
    fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<K, T>>>, command: RootComponentEditCommand<K, T>) {
        let edit = Arc::clone(&self.edit);
        let target = target.clone();
        self.handle.spawn(async move {
            if let Err(err) = edit.edit(&target, command).await {
                eprintln!("Failed to edit root component: {:?}", err);
            }
        });
    }

    fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<K, T>>>, target: &StaticPointer<TCell<K, ComponentInstance<K, T>>>, command: InstanceEditCommand<K, T>) {
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
