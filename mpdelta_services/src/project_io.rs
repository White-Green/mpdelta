use async_trait::async_trait;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::{ProjectLoader, ProjectWriter};
use mpdelta_core::project::{ProjectHandle, ProjectHandleOwned};
use std::path::Path;
use thiserror::Error;

pub struct TemporaryProjectLoader;

#[derive(Debug, Error)]
pub enum Infallible {}

#[async_trait]
impl<K, T> ProjectLoader<K, T> for TemporaryProjectLoader
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = Infallible;

    async fn load_project(&self, _: &Path) -> Result<ProjectHandleOwned<K, T>, Self::Err> {
        todo!("ProjectLoader is not implemented yet")
    }
}

pub struct TemporaryProjectWriter;

#[async_trait]
impl<K, T> ProjectWriter<K, T> for TemporaryProjectWriter
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = Infallible;

    async fn write_project(&self, _: &ProjectHandle<K, T>, _: &Path) -> Result<(), Self::Err> {
        todo!("ProjectWriter is not implemented yet")
    }
}
