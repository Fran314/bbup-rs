use super::AbstPath;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Abstract File System Error: trying to read/write data to object with unknown extension.\nPath: {path}")]
    UnknownExtension { path: String },

    #[error("Abstract File System Error: trying to perform operation on inadequate object.\nSource: {src}\nError: {err}")]
    OperationOnWrongObject { src: String, err: String },

    #[error("Abstract File System Error: inner error occurred.\nSource: {src}\n{err}")]
    Inner { src: String, err: String },

    #[error("Abstract File System Error: some error occurred.\nSource: {src}\nError: {err}")]
    Generic { src: String, err: String },
}

pub fn unkext(path: &AbstPath) -> Error {
    Error::UnknownExtension {
        path: path.to_string(),
    }
}
pub fn wrgobj<S: std::string::ToString, E: std::string::ToString>(src: S, err: E) -> Error {
    Error::OperationOnWrongObject {
        src: src.to_string(),
        err: err.to_string(),
    }
}
pub fn inerr<S: std::string::ToString, E: std::error::Error>(src: S) -> impl Fn(E) -> Error {
    move |err: E| -> Error {
        Error::Inner {
            src: src.to_string(),
            err: err.to_string(),
        }
    }
}
pub fn generr<S: std::string::ToString, T: std::string::ToString>(src: S, err: T) -> Error {
    Error::Generic {
        src: src.to_string(),
        err: err.to_string(),
    }
}
pub fn error_context<S: std::string::ToString>(context: S) -> impl Fn(&str) -> String {
    move |failure: &str| -> String { format!("{}\nFailed to {}", context.to_string(), failure) }
}
