use async_trait::async_trait;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::{ProjectLoader, ProjectWriter};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

pub struct LocalFSProjectLoader;

#[async_trait]
impl<K, T> ProjectLoader<K, T> for LocalFSProjectLoader
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = io::Error;
    type ProjectRead<'a> = File;

    async fn load_project<'a>(&'a self, path: &Path) -> Result<Self::ProjectRead<'a>, Self::Err> {
        OpenOptions::new().read(true).open(path)
    }
}

pub struct LocalFSProjectWriter;

#[async_trait]
impl<K, T> ProjectWriter<K, T> for LocalFSProjectWriter
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = io::Error;
    type ProjectWrite<'a> = File;

    async fn write_project<'a>(&'a self, path: &Path) -> Result<Self::ProjectWrite<'a>, Self::Err> {
        OpenOptions::new().write(true).create(true).truncate(true).open(path)
    }
}
