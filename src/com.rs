use crate::path::ForceFilename;

use std::{
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};

use indicatif::{ProgressBar, ProgressStyle};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum JobType {
    Pull,
    Push,
    Quit,
}

fn pb_style_from(direction: &str, name: &str) -> ProgressStyle {
    let style_path = String::from("[")
        + direction
        + "]\t"
        + name
        + "\t\t{bytes}\t{percent}%\t{bytes_per_sec}\t{elapsed_precise}";
    ProgressStyle::default_bar().template(style_path.as_str())
}
async fn update_progressbar(pb: Arc<Mutex<ProgressBar>>, bytes: Arc<Mutex<u64>>) {
    loop {
        pb.lock().unwrap().set_position(*bytes.lock().unwrap());
        tokio::time::sleep(tokio::time::Duration::from_millis(45)).await;
    }
}

pub struct ProgressWriter<'a, W: AsyncWrite + Unpin + Sync + Send> {
    pb_task_handle: tokio::task::JoinHandle<()>,
    pb: Arc<Mutex<ProgressBar>>,
    bytes_written: Arc<Mutex<u64>>,
    pub writer: &'a mut W,
}
impl<'a, W: AsyncWrite + Unpin + Sync + Send> ProgressWriter<'a, W> {
    pub fn new(writer: &'a mut W, len: u64, name: &String) -> ProgressWriter<'a, W> {
        let pb = ProgressBar::new(len);
        pb.set_style(pb_style_from("upload", name));

        let pb = Arc::new(Mutex::new(pb));
        let bytes_written = Arc::new(Mutex::new(0u64));

        let pb_task_handle = tokio::spawn(update_progressbar(pb.clone(), bytes_written.clone()));

        ProgressWriter {
            pb_task_handle,
            pb,
            bytes_written,
            writer,
        }
    }

    pub async fn finish(self) {
        self.pb.lock().unwrap().finish();
        self.pb_task_handle.abort();
    }
}
impl<'a, W: AsyncWrite + Unpin + Sync + Send> AsyncWrite for ProgressWriter<'a, W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match Pin::new(&mut self.writer).poll_write(cx, buf) {
            std::task::Poll::Ready(writer) => match writer {
                Ok(bytes) => {
                    *self.bytes_written.lock().unwrap() += bytes as u64;
                    std::task::Poll::Ready(Ok(bytes))
                }
                Err(err) => std::task::Poll::Ready(Err(err)),
            },
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

pub struct ProgressReader<'a, W: AsyncRead + Unpin + Sync + Send> {
    pb_task_handle: tokio::task::JoinHandle<()>,
    pb: Arc<Mutex<ProgressBar>>,
    bytes_read: Arc<Mutex<u64>>,
    pub reader: &'a mut W,
}
impl<'a, R: AsyncRead + Unpin + Sync + Send> ProgressReader<'a, R> {
    pub fn new(reader: &'a mut R, len: u64, name: &String) -> ProgressReader<'a, R> {
        let pb = ProgressBar::new(len);
        pb.set_style(pb_style_from("download", name));

        let pb = Arc::new(Mutex::new(pb));
        let bytes_read = Arc::new(Mutex::new(0u64));

        let pb_task_handle = tokio::spawn(update_progressbar(pb.clone(), bytes_read.clone()));

        ProgressReader {
            pb_task_handle,
            pb,
            bytes_read,
            reader,
        }
    }

    pub async fn finish(self) {
        self.pb.lock().unwrap().finish();
        self.pb_task_handle.abort();
    }
}
impl<'a, R: AsyncRead + Unpin + Sync + Send> AsyncRead for ProgressReader<'a, R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match Pin::new(&mut self.reader).poll_read(cx, buf) {
            std::task::Poll::Ready(reader) => match reader {
                Ok(()) => {
                    *self.bytes_read.lock().unwrap() += buf.filled().len() as u64;
                    std::task::Poll::Ready(Ok(()))
                }
                Err(err) => std::task::Poll::Ready(Err(err)),
            },
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

pub struct BbupCom<T: AsyncWrite + Unpin + Sync + Send, R: AsyncRead + Unpin + Sync + Send> {
    tx: T,
    rx: R,

    progress: bool,
}
impl<T: AsyncWrite + Unpin + Sync + Send, R: AsyncRead + Unpin + Sync + Send> BbupCom<T, R> {
    pub fn from_split((rx, tx): (R, T), progress: bool) -> BbupCom<T, R> {
        BbupCom { tx, rx, progress }
    }
    async fn send_status(&mut self, status: u8) -> Result<(), Error> {
        self.tx
            .write_u8(status)
            .await
            .map_err(|error| Error::WriteStatusError { error })?;

        Ok(())
    }

    async fn send_block(&mut self, content: Vec<u8>) -> Result<(), Error> {
        self.tx
            .write_u64(content.len() as u64)
            .await
            .map_err(|error| Error::WriteDataSizeError {
                sendtype: "block".to_string(),
                error,
            })?;
        self.tx
            .write_all(&content)
            .await
            .map_err(|error| Error::WriteError { error })?;
        self.tx
            .flush()
            .await
            .map_err(|error| Error::FlushTxError { error })?;

        Ok(())
    }

    pub async fn send_ok(&mut self) -> Result<(), Error> {
        self.send_status(0u8).await?;
        Ok(())
    }

    pub async fn send_error<S>(&mut self, status: u8, error: S) -> Result<(), Error>
    where
        S: std::marker::Send + std::marker::Sync + std::string::ToString,
    {
        if status == 0 {
            return Err(Error::ErrorZero);
        }
        self.send_status(status).await?;

        self.send_block(error.to_string().as_bytes().to_vec())
            .await?;

        Ok(())
    }

    pub async fn send_struct<C>(&mut self, content: C) -> Result<(), Error>
    where
        C: std::marker::Send + std::marker::Sync + Serialize,
    {
        self.send_ok().await?;
        self.send_block(
            bincode::serialize(&content).map_err(|error| Error::SerializationError { error })?,
        )
        .await?;
        self.check_ok().await?;

        Ok(())
    }

    pub async fn send_file_from(&mut self, path: &PathBuf) -> Result<(), Error> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|error| Error::OpenFileError {
                path: path.clone(),
                error,
            })?;

        self.send_ok().await?;

        let len = file
            .metadata()
            .await
            .map_err(|error| Error::MetadataError {
                path: path.clone(),
                error,
            })?
            .len();
        self.tx
            .write_u64(len)
            .await
            .map_err(|error| Error::WriteDataSizeError {
                sendtype: "file".to_string(),
                error,
            })?;

        if self.progress {
            let mut pw = ProgressWriter::new(&mut self.tx, len, &path.force_file_name());
            tokio::io::copy(&mut file, &mut pw)
                .await
                .map_err(|error| Error::BufferCopyError { error })?;

            pw.finish().await;
        } else {
            tokio::io::copy(&mut file, &mut self.tx)
                .await
                .map_err(|error| Error::BufferCopyError { error })?;
        }

        Ok(())
    }

    async fn get_block(&mut self) -> Result<Vec<u8>, Error> {
        let len = self
            .rx
            .read_u64()
            .await
            .map_err(|error| Error::ReadDataSizeError {
                gettype: "struct".to_string(),
                error,
            })?;

        let mut buffer = vec![0u8; len as usize];
        self.rx
            .read_exact(&mut buffer)
            .await
            .map_err(|error| Error::ReadError { error })?;
        Ok(buffer)
    }

    pub async fn check_ok(&mut self) -> Result<(), Error> {
        let status = self
            .rx
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

    pub async fn get_struct<'a, S>(&mut self) -> Result<S, Error>
    where
        S: std::marker::Send + std::marker::Sync + DeserializeOwned,
    {
        self.check_ok().await?;
        let buffer = self.get_block().await?;
        self.send_ok().await?;
        Ok(bincode::deserialize::<S>(&buffer[..])
            .map_err(|error| Error::DeserializationError { error })?)
    }

    pub async fn get_file_to(&mut self, path: &PathBuf) -> Result<(), Error> {
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
            .rx
            .read_u64()
            .await
            .map_err(|error| Error::ReadDataSizeError {
                gettype: "file".to_string(),
                error,
            })?;

        if self.progress {
            let pw = ProgressReader::new(&mut self.rx, len, &path.force_file_name());
            let mut handle = pw.take(len);
            tokio::io::copy(&mut handle, &mut file)
                .await
                .map_err(|error| Error::BufferCopyError { error })?;
            file.flush()
                .await
                .map_err(|error| Error::FlushFileError { error })?;

            handle.into_inner().finish().await;
        } else {
            let mut handle = (&mut self.rx).take(len);
            tokio::io::copy(&mut handle, &mut file)
                .await
                .map_err(|error| Error::BufferCopyError { error })?;

            file.flush()
                .await
                .map_err(|error| Error::FlushFileError { error })?;
        }

        Ok(())
    }
}
