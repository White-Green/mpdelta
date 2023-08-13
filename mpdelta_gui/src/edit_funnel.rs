use crate::global_ui_state::GlobalUIEventHandler;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::usecase::EditUsecase;
use qcell::TCell;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::RwLock;

pub struct EditFunnelImpl<Edit> {
    edit: Arc<Edit>,
    handle: Handle,
}

impl EditFunnelImpl<()> {
    pub fn new<K: 'static, T, Edit: EditUsecase<K, T> + 'static>(handle: Handle, edit: Arc<Edit>) -> Arc<EditFunnelImpl<Edit>> {
        Arc::new(EditFunnelImpl { edit, handle })
    }
}

pub trait EditFunnel<K: 'static, T>: Send + Sync {
    fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<K, T>>>, command: RootComponentEditCommand<K, T>);
    fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<K, T>>>, target: &StaticPointer<TCell<K, ComponentInstance<K, T>>>, command: InstanceEditCommand<K, T>);
}

impl<K: 'static, T: ParameterValueType, Edit: EditUsecase<K, T> + 'static> EditFunnel<K, T> for EditFunnelImpl<Edit> {
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
