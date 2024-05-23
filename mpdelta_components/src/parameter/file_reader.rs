use memmap2::Mmap;
use mpdelta_core::component::parameter::value::{DynEditableSingleValue, DynEditableSingleValueIdentifier, DynEditableSingleValueManager, DynEditableSingleValueMarker, NamedAny};
use mpdelta_core::component::parameter::{AbstractFile, FileAbstraction};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
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
        AbstractFile::new(FileReader::new(file, id).unwrap())
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
    file: Arc<(File, Mmap)>,
    cursor: u64,
    id: Uuid,
}

impl FileReader {
    pub fn new(file: File, id: Uuid) -> io::Result<FileReader> {
        // SAFETY: Mmap::mapがunsafeなのはmpdelta外のプロセスから同じファイルに対して変更が行われた場合などに安全性が保証されないためである
        // これに関しては通常の<File as Read>::read等も何の保証も提供しておらず、その前提で問題のない利用方法でのみ利用することにすることで解決していた
        // よって、Mmap::newを安全なものとして利用する
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(FileReader { file: Arc::new((file, mmap)), cursor: 0, id })
    }

    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        let Some(mut content) = self.file.1.get(pos as usize..) else {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "unexpected eof"));
        };
        content.read(buf)
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
            SeekFrom::End(n) => (self.file.1.len() as u64, n),
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
