use crate::component::instance::ComponentInstance;
use crate::ptr::StaticPointerOwned;
use tokio::sync::RwLock;

pub enum RootComponentEditCommand<T> {
    AddComponentInstance(StaticPointerOwned<RwLock<ComponentInstance<T>>>),
}

pub enum InstanceEditCommand {/* TODO */}
