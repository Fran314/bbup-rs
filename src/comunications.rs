use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Empty;

#[derive(Serialize, Deserialize, Debug)]
struct Message<T> {
    status: i32,
    content: T,
    comment: String,
}

pub mod syncrw {
    use serde::{de::DeserializeOwned, Serialize};
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpStream,
    };

    use crate::{comunications::Message, utils};

    pub fn write<T: Serialize, S: std::fmt::Display>(
        socket: &mut TcpStream,
        status: i32,
        content: T,
        comment: S,
    ) -> std::io::Result<()> {
        let to_send = Message {
            status,
            content,
            comment: comment.to_string(),
        };
        socket.write((serde_json::to_string(&to_send)? + "\n").as_bytes())?;
        socket.flush()?;

        Ok(())
    }

    pub fn read<'a, T: DeserializeOwned>(
        socket: &mut BufReader<TcpStream>,
        buffer: &'a mut String,
    ) -> std::io::Result<T> {
        buffer.clear();
        socket.read_line(buffer)?;
        let output: Message<T> = serde_json::from_str(buffer.as_str())?;
        match output.status {
            0 => Ok(output.content),
            _ => Err(utils::std_err(output.comment.as_str())),
        }
    }
}

pub mod asyncrw {
    use crate::{comunications::Message, utils};

    use serde::{de::DeserializeOwned, Serialize};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpStream,
    };

    pub async fn write<T: Serialize, S: std::fmt::Display>(
        socket: &mut BufReader<TcpStream>,
        status: i32,
        content: T,
        comment: S,
    ) -> std::io::Result<()> {
        let to_send = Message {
            status,
            content,
            comment: comment.to_string(),
        };
        socket
            .write((serde_json::to_string(&to_send)? + "\n").as_bytes())
            .await?;
        socket.flush().await?;

        Ok(())
    }

    pub async fn read<'a, T: DeserializeOwned>(
        socket: &mut BufReader<TcpStream>,
        buffer: &'a mut String,
    ) -> std::io::Result<T> {
        buffer.clear();
        socket.read_line(buffer).await?;
        let output: Message<T> = serde_json::from_str(buffer.as_str())?;
        match output.status {
            0 => Ok(output.content),
            _ => Err(utils::std_err(output.comment.as_str())),
        }
    }
}
