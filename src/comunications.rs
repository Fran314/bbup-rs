use std::path::PathBuf;

use async_trait::async_trait;
use serde::{
    de::DeserializeOwned,
    // Deserialize,
    Serialize,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use thiserror::Error;
#[derive(Error, Debug)]
pub enum Error {
    #[error("Serialization error: could not serialize content\n\terror: {error:?}")]
    SerializationError { error: bincode::Error },

    #[error("Deserialization error: could not deserialize binary\n\terror: {error:?}")]
    DeserializationError { error: bincode::Error },

    #[error("Comunication error: could not send status\n\terror: {error:?}")]
    WriteStatusError { error: std::io::Error },
    #[error("Comunication error: could not get status\n\terror: {error:?}")]
    ReadStatusError { error: std::io::Error },
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

    #[error("Write error: could not send data\n\terror: {error:?}")]
    WriteError { error: std::io::Error },
    #[error("Read error: could not get data\n\terror: {error:?}")]
    ReadError { error: std::io::Error },

    #[error("Recieved message with bad statusn\n\tstatus: {status}\n\terror: {error}")]
    BadStatus { status: u8, error: String },

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

    #[error("Called send error but the status is 0 (not a valid error status)")]
    ErrorZero,

    #[error("Generic error {0:#?}")]
    GenericError(String),
}

// #[derive(Serialize, Deserialize, Debug)]
// pub struct Empty;

// #[derive(Serialize, Deserialize, Debug)]
// struct Message<T> {
//     status: i32,
//     content: T,
//     comment: String,
// }

#[async_trait]
pub trait BbupWrite {
    async fn send_ok(&mut self) -> Result<(), Error>;
    async fn send_error<T>(&mut self, status: u8, error: T) -> Result<(), Error>
    where
        T: std::marker::Send + std::marker::Sync + std::string::ToString;

    async fn send_block(&mut self, content: Vec<u8>) -> Result<(), Error>;

    async fn send_struct<T>(&mut self, content: T) -> Result<(), Error>
    where
        T: std::marker::Send + std::marker::Sync + Serialize;

    async fn send_file_from(&mut self, path: &PathBuf) -> Result<(), Error>;
}
#[async_trait]
pub trait BbupRead {
    async fn check_ok(&mut self) -> Result<(), Error>;

    async fn get_block(&mut self) -> Result<Vec<u8>, Error>;

    async fn get_struct<'a, T>(&mut self) -> Result<T, Error>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned;

    async fn get_file_to(&mut self, path: &PathBuf) -> Result<(), Error>;
}

#[async_trait]
impl BbupWrite for tokio::net::tcp::OwnedWriteHalf {
    async fn send_ok(&mut self) -> Result<(), Error> {
        // Send OK status
        self.write_u8(0u8)
            .await
            .map_err(|error| Error::WriteStatusError { error })?;

        Ok(())
    }

    async fn send_error<T>(&mut self, status: u8, error: T) -> Result<(), Error>
    where
        T: std::marker::Send + std::marker::Sync + std::string::ToString,
    {
        // Send status
        if status == 0 {
            return Err(Error::ErrorZero);
        }
        self.write_u8(status)
            .await
            .map_err(|error| Error::WriteStatusError { error })?;

        self.send_block(error.to_string().as_bytes().to_vec())
            .await?;

        Ok(())
    }

    async fn send_block(&mut self, content: Vec<u8>) -> Result<(), Error> {
        self.write_u64(content.len() as u64)
            .await
            .map_err(|error| Error::WriteDataSizeError {
                sendtype: "block".to_string(),
                error,
            })?;
        self.write_all(&content)
            .await
            .map_err(|error| Error::WriteError { error })?;
        self.flush()
            .await
            .map_err(|error| Error::FlushTxError { error })?;

        Ok(())
    }

    async fn send_struct<T>(&mut self, content: T) -> Result<(), Error>
    where
        T: std::marker::Send + std::marker::Sync + Serialize,
    {
        self.send_ok().await?;
        self.send_block(
            bincode::serialize(&content).map_err(|error| Error::SerializationError { error })?,
        )
        .await?;

        Ok(())
    }

    async fn send_file_from(&mut self, path: &PathBuf) -> Result<(), Error> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|error| Error::OpenFileError {
                path: path.clone(),
                error,
            })?;

        self.send_ok().await?;

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
    async fn check_ok(&mut self) -> Result<(), Error> {
        let status = self
            .read_u8()
            .await
            .map_err(|error| Error::ReadStatusError { error })?;

        match status {
            0 => Ok(()),
            val => Err(Error::BadStatus {
                status: val,
                error: String::from_utf8_lossy(&self.get_block().await?).to_string(),
            }),
        }
    }

    async fn get_block(&mut self) -> Result<Vec<u8>, Error> {
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
            .map_err(|error| Error::ReadError { error })?;
        Ok(buffer)
    }

    async fn get_struct<'a, T>(&mut self) -> Result<T, Error>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned,
    {
        self.check_ok().await?;
        let buffer = self.get_block().await?;
        Ok(bincode::deserialize::<T>(&buffer[..])
            .map_err(|error| Error::DeserializationError { error })?)
    }

    async fn get_file_to(&mut self, path: &PathBuf) -> Result<(), Error> {
        self.check_ok().await?;
        std::fs::create_dir_all(path.parent().ok_or(Error::GenericError(
            "unable to get parent of file".to_string(),
        ))?)
        .map_err(|_| Error::GenericError("unable to create all dirs to file".to_string()))?;
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
