use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Comunications Error: inner error occurred.\nSource: {src}\n{err}")]
    InnerError { src: String, err: String },

    #[error("Comunications Error: some error occurred.\nSource: {src}\nError: {err}")]
    GenericError { src: String, err: String },
}

pub fn inerr<S: std::string::ToString, E: std::error::Error>(src: S) -> impl Fn(E) -> Error {
    move |err: E| -> Error {
        Error::InnerError {
            src: src.to_string(),
            err: err.to_string(),
        }
    }
}
pub fn generr<S: std::string::ToString, T: std::string::ToString>(src: S, err: T) -> Error {
    Error::GenericError {
        src: (src).to_string().clone(),
        err: err.to_string(),
    }
}
pub fn error_context<S: std::string::ToString, T: std::string::ToString>(
    context: S,
) -> impl Fn(T) -> String {
    move |failure: T| -> String {
        format!("{}\nFailed to {}", context.to_string(), failure.to_string())
    }
}

pub struct BbupCom {
    pub tx: tokio::net::tcp::OwnedWriteHalf,
    pub rx: tokio::net::tcp::OwnedReadHalf,

    pub progress: bool,
}
impl BbupCom {
    pub fn from(socket: tokio::net::TcpStream, progress: bool) -> BbupCom {
        let (rx, tx) = socket.into_split();
        BbupCom { tx, rx, progress }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum JobType {
    Pull,
    Push,
    Quit,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Query {
    Object(Querable, PathBuf),
    // FileAt(PathBuf),
    // SymLinkAt(PathBuf),
    Stop,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Querable {
    File,
    SymLink,
}
