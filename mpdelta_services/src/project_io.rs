use async_trait::async_trait;
use mpdelta_core::core::{ProjectLoader, ProjectWriter};
use mpdelta_core::project::Project;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use std::path::Path;
use thiserror::Error;
use tokio::sync::RwLock;

pub struct TemporaryProjectLoader;

#[derive(Debug, Error)]
pub enum Infallible {}

#[async_trait]
impl<T> ProjectLoader<T> for TemporaryProjectLoader {
    type Err = Infallible;

    async fn load_project(&self, _: &Path) -> Result<StaticPointerOwned<RwLock<Project<T>>>, Self::Err> {
        todo!("ProjectLoader is not implemented yet")
    }
}

pub struct TemporaryProjectWriter;

#[async_trait]
impl<T> ProjectWriter<T> for TemporaryProjectWriter {
    type Err = Infallible;

    async fn write_project(&self, _: &StaticPointer<RwLock<Project<T>>>, _: &Path) -> Result<(), Self::Err> {
        todo!("ProjectWriter is not implemented yet")
    }
}