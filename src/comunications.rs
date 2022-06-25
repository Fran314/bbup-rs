use std::path::PathBuf;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use thiserror::Error;
#[derive(Error, Debug)]
pub enum Error {
    #[error("Serialization error: could not serialize content\n\terror: {error:?}")]
    SerializationError { error: bincode::Error },

    #[error("Deserialization error: could not deserialize binary\n\terror: {error:?}")]
    DeserializationError { error: bincode::Error },

    #[error("Comunication error: could not send the size of {sendtype}\n\terror: {error:?}")]
    WriteDataSizeError {
        sendtype: String,
        error: std::io::Error,
    },
    #[error("Comunication error: could not get the size of {gettype}\n\terror: {error:?}")]
    ReadDataSizeError {
        gettype: String,
        error: std::io::Error,
    },

    #[error("Struct write error: could not send struct\n\terror: {error:?}")]
    WriteStructError { error: std::io::Error },
    #[error("Struct read error: could not get struct\n\terror: {error:?}")]
    ReadStructError { error: std::io::Error },

    #[error("Recieved message with bad statusn\n\tstatus: {status}\n\terror: {error}")]
    BadStatus { status: i32, error: String },

    #[error("Flush TX error: could not flush writing buffer\n\terror: {error:?}")]
    FlushTxError { error: std::io::Error },

    #[error("Open file error: could not open file\n\tpath: {path:?}\n\terror: {error:?}")]
    OpenFileError {
        path: PathBuf,
        error: std::io::Error,
    },

    #[error(
        "Metadata error: could not get metadata of file\n\tpath: {path:?}\n\terror: {error:?}"
    )]
    MetadataError {
        path: PathBuf,
        error: std::io::Error,
    },

    #[error("Buffer copy error: could not copy buffer\n\terror: {error:?}")]
    BufferCopyError { error: std::io::Error },

    #[error("Flush file error: could not flush file\n\terror: {error:?}")]
    FlushFileError { error: std::io::Error },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Empty;

#[derive(Serialize, Deserialize, Debug)]
struct Message<T> {
    status: i32,
    content: T,
    comment: String,
}

#[async_trait]
pub trait BbupWrite {
    async fn send_struct<T, S>(&mut self, status: i32, content: T, comment: S) -> Result<(), Error>
    where
        T: std::marker::Send + std::marker::Sync + Serialize,
        S: std::marker::Send + std::marker::Sync + std::fmt::Display;

    async fn send_file_from(&mut self, path: &PathBuf) -> Result<(), Error>;
}
#[async_trait]
pub trait BbupRead {
    async fn get_struct<'a, T>(&mut self) -> Result<T, Error>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned + std::fmt::Debug;

    async fn get_file_to(&mut self, path: &PathBuf) -> Result<(), Error>;
}

#[async_trait]
impl BbupWrite for tokio::net::tcp::OwnedWriteHalf {
    async fn send_struct<T, S>(&mut self, status: i32, content: T, comment: S) -> Result<(), Error>
    where
        T: std::marker::Send + std::marker::Sync + Serialize,
        S: std::marker::Send + std::marker::Sync + std::fmt::Display,
    {
        let to_send = Message {
            status,
            content,
            comment: comment.to_string(),
        };
        let serialized =
            bincode::serialize(&to_send).map_err(|error| Error::SerializationError { error })?;
        self.write_u64(serialized.len() as u64)
            .await
            .map_err(|error| Error::WriteDataSizeError {
                sendtype: "struct".to_string(),
                error,
            })?;
        self.write_all(&serialized)
            .await
            .map_err(|error| Error::WriteStructError { error })?;
        self.flush()
            .await
            .map_err(|error| Error::FlushTxError { error })?;

        Ok(())
    }

    async fn send_file_from(&mut self, path: &PathBuf) -> Result<(), Error> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|error| Error::OpenFileError {
                path: path.clone(),
                error,
            })?;

        self.write_u64(
            file.metadata()
                .await
                .map_err(|error| Error::MetadataError {
                    path: path.clone(),
                    error,
                })?
                .len(),
        )
        .await
        .map_err(|error| Error::WriteDataSizeError {
            sendtype: "file".to_string(),
            error,
        })?;

        tokio::io::copy(&mut file, &mut self)
            .await
            .map_err(|error| Error::BufferCopyError { error })?;

        Ok(())
    }
}

#[async_trait]
impl BbupRead for tokio::net::tcp::OwnedReadHalf {
    async fn get_struct<'a, T>(&mut self) -> Result<T, Error>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned + std::fmt::Debug,
    {
        let len = self
            .read_u64()
            .await
            .map_err(|error| Error::ReadDataSizeError {
                gettype: "struct".to_string(),
                error,
            })?;

        let mut buffer = vec![0u8; len as usize];
        self.read_exact(&mut buffer)
            .await
            .map_err(|error| Error::ReadStructError { error })?;

        let output: Message<T> = bincode::deserialize(&buffer[..])
            .map_err(|error| Error::DeserializationError { error })?;
        match output.status {
            0 => Ok(output.content),
            val => Err(Error::BadStatus {
                status: val,
                error: output.comment,
            }),
        }
    }

    async fn get_file_to(&mut self, path: &PathBuf) -> Result<(), Error> {
        let mut file =
            tokio::fs::File::create(path)
                .await
                .map_err(|error| Error::OpenFileError {
                    path: path.clone(),
                    error,
                })?;
        let len = self
            .read_u64()
            .await
            .map_err(|error| Error::ReadDataSizeError {
                gettype: "file".to_string(),
                error,
            })?;
        let mut handle = self.take(len);
        tokio::io::copy(&mut handle, &mut file)
            .await
            .map_err(|error| Error::BufferCopyError { error })?;
        file.flush()
            .await
            .map_err(|error| Error::FlushFileError { error })?;
        Ok(())
    }
}
