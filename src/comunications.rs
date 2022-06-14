use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Basic {
    pub content: String,
}
impl Basic {
    pub fn new(content: &str) -> Basic {
        Basic {
            content: content.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LastCommit {
    pub commit_id: String,
}
impl LastCommit {
    pub fn new(commit_id: &str) -> LastCommit {
        LastCommit {
            commit_id: commit_id.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Commit {
    pub delta: Vec<(u8, u8, String)>,
}
impl Commit {
    pub fn new(delta: Vec<(u8, u8, String)>) -> Commit {
        Commit { delta }
    }
}

pub mod syncrw {
    use serde::{Deserialize, Serialize};
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpStream,
    };

    pub fn write<T: Serialize>(socket: &mut TcpStream, content: T) -> std::io::Result<()> {
        socket.write((serde_json::to_string(&content)? + "\n").as_bytes())?;
        socket.flush()?;

        Ok(())
    }

    pub fn read<'a, T: Deserialize<'a>>(
        socket: &mut BufReader<TcpStream>,
        buffer: &'a mut String,
    ) -> std::io::Result<T> {
        buffer.clear();
        socket.read_line(buffer)?;
        let output: T = serde_json::from_str(buffer.as_str())?;
        Ok(output)
    }
}

pub mod asyncrw {
    use serde::{Deserialize, Serialize};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpStream,
    };
    pub async fn write<T: Serialize>(
        socket: &mut BufReader<TcpStream>,
        content: T,
    ) -> std::io::Result<()> {
        socket
            .write((serde_json::to_string(&content)? + "\n").as_bytes())
            .await?;
        socket.flush().await?;

        Ok(())
    }

    pub async fn read<'a, T: Deserialize<'a>>(
        socket: &mut BufReader<TcpStream>,
        buffer: &'a mut String,
    ) -> std::io::Result<T> {
        buffer.clear();
        socket.read_line(buffer).await?;
        let output: T = serde_json::from_str(buffer.as_str())?;
        Ok(output)
    }
}
