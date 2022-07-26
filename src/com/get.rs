use crate::{
    fs::{self, OsStrExt, PathExt},
    hash::{self, Hash},
};

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{
    bbupcom::{error_context, generr, inerr, Error, Querable, Query},
    BbupCom, ProgressReader,
};

impl BbupCom {
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

    pub async fn get_file_to<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
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
        Ok(())
    }

    pub async fn query_files(
        &mut self,
        queries: Vec<(Querable, PathBuf, Hash)>,
        endpoint: &PathBuf,
    ) -> Result<(), Error> {
        let errmsg = String::from("could not query files and symlinks");
        let errctx = error_context(errmsg.clone());
        for (querable, rel_path, hash) in queries {
            let path = endpoint.join(rel_path.clone());
            match querable {
                Querable::File => {
                    self.send_struct(Query::Object(Querable::File, rel_path.clone()))
                        .await
                        .map_err(inerr(errctx(format!(
                            "ask query for file at path {path:?}"
                        ))))?;

                    self.get_file_to(&path)
                        .await
                        .map_err(inerr(errctx(format!("query file at path {path:?}"))))?;

                    let file = fs::read_file(&path).map_err(inerr(errctx(format!(
                        "open file to check hash at path {path:?}"
                    ))))?;

                    if hash
                        != hash::hash_stream(file)
                            .map_err(inerr(errctx(format!("hash file content at path {path:?}"))))?
                    {
                        return Err(generr(errmsg, format!("hash of the file recieved (at path {path:?}) does not match the hash given")));
                    }
                }
                Querable::SymLink => {
                    self.send_struct(Query::Object(Querable::SymLink, rel_path.clone()))
                        .await
                        .map_err(inerr(errctx(format!(
                            "ask query for symlink at path {path:?}"
                        ))))?;

                    let endpoint: PathBuf = self.get_struct().await.map_err(inerr(errctx(
                        format!("query symlink's endpoint at path {path:?}"),
                    )))?;

                    if hash != hash::hash_bytes(endpoint.force_to_string().as_bytes()) {
                        return Err(generr(errmsg, format!("hash of the symlink recieved (at path {path:?}) does not match the hash given")));
                    }
                    fs::create_symlink(&path, endpoint).map_err(inerr(errctx(format!(
                        "create queried symlink at path {path:?}"
                    ))))?;
                }
            }
        }
        self.send_struct(Query::Stop)
            .await
            .map_err(inerr(errctx(format!("send query stop signal"))))?;

        Ok(())
    }
}
