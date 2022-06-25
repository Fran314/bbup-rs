use std::path::PathBuf;

use crate::utils;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    async fn send_struct<T, S>(
        &mut self,
        status: i32,
        content: T,
        comment: S,
    ) -> std::io::Result<()>
    where
        T: std::marker::Send + std::marker::Sync + Serialize,
        S: std::marker::Send + std::marker::Sync + std::fmt::Display;

    async fn send_file_from(&mut self, path: &PathBuf) -> std::io::Result<()>;
}
#[async_trait]
pub trait BbupRead {
    async fn get_struct<'a, T>(&mut self) -> std::io::Result<T>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned + std::fmt::Debug;

    async fn get_file_to(&mut self, path: &PathBuf) -> std::io::Result<()>;
}

#[async_trait]
impl BbupWrite for tokio::net::tcp::OwnedWriteHalf {
    async fn send_struct<T, S>(
        &mut self,
        status: i32,
        content: T,
        comment: S,
    ) -> std::io::Result<()>
    where
        T: std::marker::Send + std::marker::Sync + Serialize,
        S: std::marker::Send + std::marker::Sync + std::fmt::Display,
    {
        let to_send = Message {
            status,
            content,
            comment: comment.to_string(),
        };
        let serialized = bincode::serialize(&to_send).map_err(utils::to_io_err)?;
        self.write_u64(serialized.len() as u64).await?;
        self.write_all(&serialized).await?;
        self.flush().await?;

        Ok(())
    }

    async fn send_file_from(&mut self, path: &PathBuf) -> std::io::Result<()> {
        let mut file = tokio::fs::File::open(path).await?;
        self.write_u64(file.metadata().await?.len()).await?;
        tokio::io::copy(&mut file, &mut self).await?;
        Ok(())
    }
}

#[async_trait]
impl BbupRead for tokio::net::tcp::OwnedReadHalf {
    async fn get_struct<'a, T>(&mut self) -> std::io::Result<T>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned + std::fmt::Debug,
    {
        let len = self.read_u64().await?;
        let mut buffer = vec![0u8; len as usize];
        self.read_exact(&mut buffer).await?;

        let output: Message<T> = bincode::deserialize(&buffer[..]).map_err(utils::to_io_err)?;
        match output.status {
            0 => Ok(output.content),
            _ => Err(utils::std_err(output.comment.as_str())),
        }
    }

    async fn get_file_to(&mut self, path: &PathBuf) -> std::io::Result<()> {
        let mut file = tokio::fs::File::create(path).await?;
        let len = self.read_u64().await?;
        let mut handle = self.take(len);
        tokio::io::copy(&mut handle, &mut file).await?;
        file.flush().await?;
        Ok(())
    }
}
