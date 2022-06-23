use crate::structs;

use std::ffi::OsStr;
use std::fs;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug)]
pub enum SerdeError {
    SerdeJsonError(serde_json::Error),
    SerdeYamlError(serde_yaml::Error),
}
#[derive(Error, Debug)]
pub enum Error {
    #[error("Trying to read data with unknown extension on path {path:?}")]
    UnknownExtension { path: PathBuf },

    #[error("Read error: could not read file\n\tpath: {path:?}\n\terror: {error:?}")]
    ReadError {
        path: PathBuf,
        error: std::io::Error,
    },

    #[error("Write error: could not write file\n\tpath: {path:?}\n\terror: {error:?}")]
    WriteError {
        path: PathBuf,
        error: std::io::Error,
    },

    #[error(
        "Serialization error: could not serialize content\n\tpath: {path:?}\n\terror: {error:?}"
    )]
    SerializationError { path: PathBuf, error: SerdeError },

    #[error("Deserialization error: could not deserialize content\n\tpath: {path:?}\n\terror: {error:?}")]
    DeserializationError {
        path: PathBuf,
        error: std::io::Error,
    },
}

//--- SERVER STUFF ---//
pub type CommitList = Vec<structs::Commit>;
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfing {
    pub archive_root: PathBuf,
}
//--- ---//

//--- CLIENT STUFF ---//
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientConfig {
    pub settings: structs::ClientSettings,
    pub links: Vec<String>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkConfig {
    pub link_type: structs::LinkType,
    pub endpoint: PathBuf,
    pub exclude_list: Vec<String>,
}
//--- ---//

pub fn load<T: DeserializeOwned>(path: &PathBuf) -> std::result::Result<T, Error> {
    let serialized = fs::read_to_string(path).map_err(|error| Error::ReadError {
        path: path.clone(),
        error,
    })?;
    let content: T = match path.extension().and_then(OsStr::to_str) {
        Some(ext) => match ext.to_ascii_lowercase().as_str() {
            "json" => {
                serde_json::from_str(&serialized).map_err(|error| Error::SerializationError {
                    path: path.clone(),
                    error: SerdeError::SerdeJsonError(error),
                })
            }
            "yaml" => {
                serde_yaml::from_str(&serialized).map_err(|error| Error::SerializationError {
                    path: path.clone(),
                    error: SerdeError::SerdeYamlError(error),
                })
            }
            _ => Err(Error::UnknownExtension { path: path.clone() }),
        },
        None => Err(Error::UnknownExtension { path: path.clone() }),
    }?;
    Ok(content)
}
pub fn save<T: Serialize>(path: &PathBuf, content: &T) -> std::result::Result<(), Error> {
    let serialized = match path.extension().and_then(OsStr::to_str) {
        Some(ext) => match ext.to_ascii_lowercase().as_str() {
            "json" => serde_json::to_string(content).map_err(|error| Error::SerializationError {
                path: path.clone(),
                error: SerdeError::SerdeJsonError(error),
            }),
            "yaml" => serde_yaml::to_string(content).map_err(|error| Error::SerializationError {
                path: path.clone(),
                error: SerdeError::SerdeYamlError(error),
            }),
            _ => Err(Error::UnknownExtension { path: path.clone() }),
        },
        None => Err(Error::UnknownExtension { path: path.clone() }),
    }?;
    fs::write(path, serialized).map_err(|error| Error::WriteError {
        path: path.clone(),
        error,
    })?;
    Ok(())
}
