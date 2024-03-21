use mpdelta_core::component::parameter::value::{DynEditableSingleValue, DynEditableSingleValueIdentifier, DynEditableSingleValueManager, DynEditableSingleValueMarker, NamedAny};
use mpdelta_core::component::parameter::{AbstractFile, FileAbstraction};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::{io, os};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReaderParam {
    path: PathBuf,
}

impl FileReaderParam {
    pub fn new(path: PathBuf) -> FileReaderParam {
        FileReaderParam { path }
    }
}

impl DynEditableSingleValueMarker for FileReaderParam {
    type Out = AbstractFile;

    fn manager(&self) -> &dyn DynEditableSingleValueManager<Self::Out> {
        &FileReaderParamManager
    }

    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
        &mut self.path
    }

    fn get_value(&self) -> Self::Out {
        let Ok(file) = File::open(&self.path) else {
            return AbstractFile::default();
        };
        let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, &b"file://".iter().chain(self.path.as_os_str().as_encoded_bytes()).copied().collect::<Vec<_>>());
        AbstractFile::new(FileReader::new(file, id))
    }
}

pub struct FileReaderParamManager;

impl DynEditableSingleValueManager<AbstractFile> for FileReaderParamManager {
    fn identifier(&self) -> DynEditableSingleValueIdentifier {
        DynEditableSingleValueIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("FileReaderParam"),
        }
    }

    fn deserialize(&self, deserializer: &mut dyn erased_serde::Deserializer) -> Result<DynEditableSingleValue<AbstractFile>, erased_serde::Error> {
        let value: FileReaderParam = erased_serde::deserialize(deserializer)?;
        Ok(DynEditableSingleValue::new(value))
    }
}

#[derive(Clone)]
pub struct FileReader {
    file: Arc<File>,
    cursor: u64,
    id: Uuid,
}

impl FileReader {
    pub fn new(file: File, id: Uuid) -> FileReader {
        FileReader { file: Arc::new(file), cursor: 0, id }
    }

    #[cfg(target_os = "windows")]
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        os::windows::fs::FileExt::seek_read(&*self.file, buf, pos)
    }

    #[cfg(not(target_os = "windows"))]
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        os::unix::fs::FileExt::read_at(&*self.file, buf, pos)
    }
}

impl Read for FileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = self.read_at(self.cursor, buf)?;
        self.cursor += len as u64;
        Ok(len)
    }
}

impl Seek for FileReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.cursor = n;
                return Ok(n);
            }
            SeekFrom::End(n) => (self.file.metadata()?.len(), n),
            SeekFrom::Current(n) => (self.cursor, n),
        };
        match base_pos.checked_add_signed(offset) {
            Some(n) => {
                self.cursor = n;
                Ok(self.cursor)
            }
            None => Err(io::Error::new(ErrorKind::InvalidInput, "invalid seek to a negative or overflowing position")),
        }
    }
}

impl FileAbstraction for FileReader {
    fn identifier(&self) -> Uuid {
        self.id
    }

    fn duplicate(&self) -> Box<dyn FileAbstraction> {
        Box::new(self.clone())
    }
}
