use serde::Serialize;

use super::{
    bbupcom::{error_context, generr, inerr, Error},
    BbupCom, ProgressWriter,
};

use abst_fs::{self as fs, AbstPath};

use tokio::io::AsyncWriteExt;

impl BbupCom {
    async fn send_status(&mut self, status: u8) -> Result<(), Error> {
        let errctx = error_context(format!("could not send status {status}"));
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
        let errmsg = format!("could not send error status {status}");
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

    pub async fn send_file_from(&mut self, path: &AbstPath) -> Result<(), Error> {
        let errctx = error_context(format!("could not send file at path {path}"));
        let mut file = fs::async_read_file(path)
            .await
            .map_err(inerr(errctx("async read the file")))?;

        self.send_ok()
            .await
            .map_err(inerr(errctx("send ok status")))?;

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
                Some(val) => val,
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

    // pub async fn supply_mapped_files(
    //     &mut self,
    //     map: HashMap<AbstPath, AbstPath>,
    //     source: &AbstPath,
    // ) -> Result<(), Error> {
    //     let errmsg = String::from("could not supply files and symlinks");
    //     let errctx = error_context(errmsg.clone());
    //     loop {
    //         let query: Query = self
    //             .get_struct()
    //             .await
    //             .map_err(inerr(errctx("get query".to_string())))?;
    //         match query {
    //             Query::Stop => break,
    //             Query::File(rel_path) => match map.get(&rel_path) {
    //                 None => {
    //                     self.send_error(1, "quered file at path not allowed")
    //                         .await
    //                         .map_err(inerr(errctx(format!(
    //                             "propagate not allowed path at {rel_path}"
    //                         ))))?;
    //                     return Err(generr(
    //                         errmsg,
    //                         format!(
    //                             "other party tried to query a non queryable path at {rel_path}"
    //                         ),
    //                     ));
    //                 }
    //                 Some(mapped_path) => {
    //                     let path = source.append(mapped_path);
    //                     self.send_file_from(&path)
    //                         .await
    //                         .map_err(inerr(errctx(format!("send quered file at path {path}"))))?;
    //                 }
    //             },
    //         }
    //     }
    //
    //     Ok(())
    // }
    //
    // pub async fn supply_files(
    //     &mut self,
    //     queryable: Vec<AbstPath>,
    //     source: &AbstPath,
    // ) -> Result<(), Error> {
    //     let mut map = HashMap::new();
    //     for path in queryable {
    //         map.insert(path.clone(), path);
    //     }
    //     self.supply_mapped_files(map, source).await
    // }
}
