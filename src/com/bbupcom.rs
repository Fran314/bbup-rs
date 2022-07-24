use super::{ProgressReader, ProgressWriter};

use crate::{
    fs::{self, OsStrExt},
    hash::{self, Hash},
};

use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Comunications Error: inner error occurred.\nSource: {src}\n{err}")]
    InnerError { src: String, err: String },

    #[error("Comunications Error: some error occurred.\nSource: {src}\nError: {err}")]
    GenericError { src: String, err: String },
}

fn inerr<S: std::string::ToString, E: std::error::Error>(src: S) -> impl Fn(E) -> Error {
    move |err: E| -> Error {
        Error::InnerError {
            src: src.to_string(),
            err: err.to_string(),
        }
    }
}
fn generr<S: std::string::ToString, T: std::string::ToString>(src: S, err: T) -> Error {
    Error::GenericError {
        src: (src).to_string().clone(),
        err: err.to_string(),
    }
}
fn error_context<S: std::string::ToString>(context: S) -> impl Fn(&str) -> String {
    move |failure: &str| -> String { format!("{}\nFailed to {}", context.to_string(), failure) }
}

pub struct BbupCom {
    tx: tokio::net::tcp::OwnedWriteHalf,
    rx: tokio::net::tcp::OwnedReadHalf,

    progress: bool,
}
impl BbupCom {
    pub fn from(socket: tokio::net::TcpStream, progress: bool) -> BbupCom {
        let (rx, tx) = socket.into_split();
        BbupCom { tx, rx, progress }
    }

    //--- WRITE ---//
    async fn send_status(&mut self, status: u8) -> Result<(), Error> {
        let errctx = error_context(format!("could not send status {}", status));
        self.tx
            .write_u8(status)
            .await
            .map_err(inerr(errctx("send status")))
    }

    async fn send_block(&mut self, content: Vec<u8>) -> Result<(), Error> {
        let errctx = error_context("could not send block");
        self.tx
            .write_u64(content.len() as u64)
            .await
            .map_err(inerr(errctx("send length of block")))?;
        self.tx
            .write_all(&content)
            .await
            .map_err(inerr(errctx("send block body")))?;
        self.tx.flush().await.map_err(inerr(errctx("flush data")))?;

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
        let errmsg = format!("could not send error status {}", status);
        let errctx = error_context(errmsg.clone());
        if status == 0 {
            return Err(generr(errmsg, "status 0 is not an error status!"));
        }
        self.send_status(status)
            .await
            .map_err(inerr(errctx("send error code")))?;

        self.send_block(
            bincode::serialize(&error.to_string())
                .map_err(inerr(errctx("serialize error message")))?,
        )
        .await
        .map_err(inerr(errctx("send error message")))?;

        Ok(())
    }

    pub async fn send_struct<C>(&mut self, content: C) -> Result<(), Error>
    where
        C: std::marker::Send + std::marker::Sync + Serialize,
    {
        let errctx = error_context(format!(
            "could not send struct of type {}",
            std::any::type_name::<C>()
        ));
        self.send_ok().await.map_err(inerr(errctx("send ok")))?;
        self.send_block(bincode::serialize(&content).map_err(inerr("serialize struct"))?)
            .await
            .map_err(inerr("send serialized struct"))?;
        self.check_ok()
            .await
            .map_err(inerr("get ok confirmation"))?;

        Ok(())
    }

    pub async fn send_file_from<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        let path = path.as_ref().to_path_buf();
        let errctx = error_context(format!("could not send file at path {:?}", path));
        let mut file = fs::async_read_file(&path)
            .await
            .map_err(inerr(errctx("async read the file")))?;

        self.send_ok().await?;

        let len = file
            .metadata()
            .await
            .map_err(inerr(errctx("read file metadata to retrieve file length")))?
            .len();
        self.tx
            .write_u64(len)
            .await
            .map_err(inerr(errctx("send length of file")))?;

        if self.progress {
            let name = match path.file_name() {
                Some(val) => val.force_to_string(),
                None => String::from("[invalid filename]"),
            };
            let mut pw = ProgressWriter::new(&mut self.tx, len, &name);
            tokio::io::copy(&mut file, &mut pw)
                .await
                .map_err(inerr(errctx("copy file content into progress writer")))?;

            pw.finish().await;
        } else {
            tokio::io::copy(&mut file, &mut self.tx)
                .await
                .map_err(inerr(errctx("copy file content into tx")))?;
        }

        Ok(())
    }
    //--- ---//

    //--- READ ---//
    pub async fn check_ok(&mut self) -> Result<(), Error> {
        let errmsg = format!("check for ok status");
        let errctx = error_context(errmsg.clone());
        let status = self
            .rx
            .read_u8()
            .await
            .map_err(inerr(errctx("read status byte")))?;

        match status {
            0 => Ok(()),
            val => {
                let serialized_errmsg = self
                    .get_block()
                    .await
                    .map_err(inerr(errctx("retrieve error message")))?;
                let received_error_message: String =
                    bincode::deserialize(&serialized_errmsg[..])
                        .map_err(inerr(errctx("deserialize error message")))?;
                Err(generr(
                    errmsg,
                    format!(
                        "received bad status ({}) with the following error message: {}",
                        val, received_error_message
                    ),
                ))
            }
        }
    }
    async fn get_block(&mut self) -> Result<Vec<u8>, Error> {
        let errctx = error_context("could not get block");
        let len = self
            .rx
            .read_u64()
            .await
            .map_err(inerr(errctx("get block length")))?;

        let mut buffer = vec![0u8; len as usize];
        self.rx
            .read_exact(&mut buffer)
            .await
            .map_err(inerr(errctx("get block body")))?;
        Ok(buffer)
    }

    pub async fn get_struct<'a, S>(&mut self) -> Result<S, Error>
    where
        S: std::marker::Send + std::marker::Sync + DeserializeOwned,
    {
        let errctx = error_context(format!(
            "could not get struct of type {}",
            std::any::type_name::<S>()
        ));
        self.check_ok()
            .await
            .map_err(inerr(errctx("get ok status")))?;
        let buffer = self
            .get_block()
            .await
            .map_err(inerr(errctx("get serialized struct")))?;
        match bincode::deserialize::<S>(&buffer[..]) {
            Ok(content) => {
                self.send_ok().await.map_err(inerr(errctx(
                    "send confirmation that the struct arrived correctly",
                )))?;
                Ok(content)
            }
            Err(err) => {
                self.send_error(1, "failed to deserialize the recieved block for struct")
                    .await
                    .map_err(inerr(errctx(
                        "send error status as block could not be deserialized",
                    )))?;
                Err(generr(errctx("deserialize block"), err))
            }
        }
    }

    pub async fn get_file_to<P: AsRef<Path>>(
        &mut self,
        path: P,
        supposed_hash: &Hash,
    ) -> Result<(), Error> {
        let path = path.as_ref().to_path_buf();
        let errmsg = format!("could not get file to path {:?}", path);
        let errctx = error_context(errmsg.clone());
        self.check_ok()
            .await
            .map_err(inerr(errctx("get ok status")))?;
        let mut file = fs::async_create_file(&path)
            .await
            .map_err(inerr(errctx("async create file to save content")))?;
        let len = self
            .rx
            .read_u64()
            .await
            .map_err(inerr(errctx("get file length")))?;

        if self.progress {
            let name = match path.file_name() {
                Some(val) => val.force_to_string(),
                None => String::from("[invalid filename]"),
            };
            let pw = ProgressReader::new(&mut self.rx, len, &name);
            let mut handle = pw.take(len);
            tokio::io::copy(&mut handle, &mut file)
                .await
                .map_err(inerr(errctx("copy progress reader to file content")))?;

            handle.into_inner().finish().await;
        } else {
            let mut handle = (&mut self.rx).take(len);
            tokio::io::copy(&mut handle, &mut file)
                .await
                .map_err(inerr(errctx("copy rx to file content")))?;
        }
        file.flush()
            .await
            .map_err(inerr(errctx("flush file content to file")))?;
        drop(file);
        let file = fs::read_file(&path)
            .map_err(inerr(errctx("read content of file to check final hash")))?;
        let actual_hash =
            hash::hash_stream(file).map_err(inerr(errctx("hash final content of file")))?;

        match supposed_hash == &actual_hash {
            true => Ok(()),
            false => Err(generr(
                errmsg,
                "received file's hash does not equal the given supposed hash",
            )),
        }
    }
}
