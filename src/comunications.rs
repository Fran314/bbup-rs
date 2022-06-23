use crate::utils;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Empty;

#[derive(Serialize, Deserialize, Debug)]
struct Message<T> {
    status: i32,
    content: T,
    comment: String,
}

#[async_trait]
pub trait BbupComunications {
    async fn send<T, S>(&mut self, status: i32, content: T, comment: S) -> std::io::Result<()>
    where
        T: std::marker::Send + std::marker::Sync + Serialize,
        S: std::marker::Send + std::marker::Sync + std::fmt::Display;

    async fn get<'a, T>(&mut self, buffer: &'a mut String) -> std::io::Result<T>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned;
}

#[async_trait]
impl BbupComunications for BufReader<TcpStream> {
    async fn send<T, S>(&mut self, status: i32, content: T, comment: S) -> std::io::Result<()>
    where
        T: std::marker::Send + std::marker::Sync + Serialize,
        S: std::marker::Send + std::marker::Sync + std::fmt::Display,
    {
        let to_send = Message {
            status,
            content,
            comment: comment.to_string(),
        };
        self.write((serde_json::to_string(&to_send)? + "\n").as_bytes())
            .await?;
        self.flush().await?;

        Ok(())
    }
    async fn get<'a, T>(&mut self, buffer: &'a mut String) -> std::io::Result<T>
    where
        T: std::marker::Send + std::marker::Sync + DeserializeOwned,
    {
        buffer.clear();
        self.read_line(buffer).await?;
        let output: Message<T> = serde_json::from_str(buffer.as_str())?;
        match output.status {
            0 => Ok(output.content),
            _ => Err(utils::std_err(output.comment.as_str())),
        }
    }
}
