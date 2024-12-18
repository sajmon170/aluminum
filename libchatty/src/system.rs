use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    str::FromStr,
    io
};

use tokio::fs::File;
use serde::{Serialize, Deserialize};
use crate::{mime::Mime, utils};

pub fn get_user_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap()
        .join("aluminum")
}

pub fn get_default_path() -> OsString {
    get_user_dir()
        .join("user.db")
        .into_os_string()
}

pub fn get_relay_path() -> PathBuf {
    get_user_dir()
        .join("relay.toml")
}

pub fn get_downloads_dir() -> PathBuf {
    dirs::download_dir().unwrap()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub name: String,
    pub size: u64,
    pub hash: blake3::Hash,
    pub filetype: Option<Mime>
}

impl FileMetadata {
    pub fn get_save_path(&self) -> PathBuf {
        get_downloads_dir().join(&self.name)
    }

    pub fn get_local_handle(&self) -> FileHandle {
        FileHandle {
            path: self.get_save_path(),
            metadata: self.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHandle {
    path: PathBuf,
    metadata: FileMetadata,
}

impl FileHandle {
    pub async fn new(path: PathBuf) -> io::Result<FileHandle> {
        let name = path.file_name()
            .unwrap()
            .to_os_string()
            .into_string()
            .unwrap();

        let (size, hash) =  {
            let mut file = File::open(&path).await?;
            let size = file.metadata().await?.len();
            let hash = utils::get_hash_from_file(&mut file).await?;

            (size, hash)
        };
        
        let cloned_path = path.clone();
        let filetype = tokio::task::spawn_blocking(move || {
            let info = infer::Infer::new();
            info.get_from_path(&cloned_path)
        })
            .await??
            .map(|x| Mime::from_str(x.mime_type()).unwrap());


        Ok(
            FileHandle {
                path,
                metadata: FileMetadata { name, size, filetype, hash }
            }
        )
    }

    pub async fn open(&self) -> io::Result<File> {
        File::open(&self.path).await
    }

    pub fn get_metadata(&self) -> &FileMetadata {
        &self.metadata
    }

    pub fn get_path(&self) -> &Path {
        &self.path.as_ref()
    }
}

pub type Hash = blake3::Hash;
