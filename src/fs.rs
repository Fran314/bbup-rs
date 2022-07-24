use std::ffi::OsStr;
use std::os::unix::prelude::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use filetime::FileTime;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use thiserror::Error;

//--- ERRORS ---//
#[derive(Error, Debug)]
pub enum Error {
    #[error("Abstract File System: trying to read/write data to object with unknown extension.\nPath: {path:?}")]
    UnknownExtension { path: PathBuf },

    #[error("Abstract File System: trying to perform operation on inadequate object.\nSource: {src}\nError: {err}")]
    OperationOnWrongObject { src: String, err: String },

    #[error("Abstract File System: inner error occurred.\nSource: {src}\n{err}")]
    InnerError { src: String, err: String },

    #[error("Abstract File System: some error occurred.\nSource: {src}\nError: {err}")]
    GenericError { src: String, err: String },
}

fn unkext<P: AsRef<Path>>(path: P) -> Error {
    Error::UnknownExtension {
        path: path.as_ref().to_path_buf(),
    }
}
fn wrgobj<S: std::string::ToString, E: std::string::ToString>(src: S, err: E) -> Error {
    Error::OperationOnWrongObject {
        src: src.to_string(),
        err: err.to_string(),
    }
}
fn inerr<S: std::string::ToString, E: std::error::Error>(src: S) -> impl Fn(E) -> Error {
    move |err: E| -> Error {
        Error::InnerError {
            src: (src).to_string().clone(),
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
//--- ---//

//--- OBJECT TYPES ---//
#[derive(PartialEq)]
pub enum ObjectType {
    File,
    SymLink,
    Dir,
}

pub trait OsStrExt {
    fn force_to_string(&self) -> String;
}
impl OsStrExt for OsStr {
    fn force_to_string(&self) -> String {
        self.to_str()
            .expect(
                format!(
                    "Broken path: could not convert from os string to valid utf8\nos string: {:?}",
                    self
                )
                .as_str(),
            )
            .to_string()
    }
}
pub trait PathExt {
    fn get_type(&self) -> ObjectType;
    fn force_to_string(&self) -> String;
}
impl PathExt for std::path::Path {
    fn get_type(&self) -> ObjectType {
        if self.is_symlink() {
            return ObjectType::SymLink;
        } else if self.is_dir() {
            return ObjectType::Dir;
        } else if self.is_file() {
            return ObjectType::File;
        } else {
            panic!(
                "Foreign file system object. Not a directory, a file nor a symlink, at path: {:?}",
                self
            )
        }
    }
    fn force_to_string(&self) -> String {
        self.as_os_str().force_to_string()
    }
}
impl PathExt for std::path::PathBuf {
    fn get_type(&self) -> ObjectType {
        self.as_path().get_type()
    }
    fn force_to_string(&self) -> String {
        self.as_path().force_to_string()
    }
}
//--- ---//

//--- SAVE & LOAD SERIALIZED DATA ---//
enum Ext {
    JSON,
    YAML,
    // BIN,
}
fn get_ext<P: AsRef<Path>>(path: P) -> Option<Ext> {
    let ext = match path.as_ref().extension().and_then(std::ffi::OsStr::to_str) {
        Some(val) => val,
        None => return None,
    };
    match ext.to_ascii_lowercase().as_str() {
        "json" => Some(Ext::JSON),
        "yaml" => Some(Ext::YAML),
        _ => None,
    }
}

/// Load the data from a file, interpreting the content of the file based on the
/// extension (see [`Ext`] for the possible extensions) of the file and deserializing
/// the content to the generic type T
pub fn load<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T, Error> {
    let errctx = error_context(format!("could not load file at path {:?}", path.as_ref()));
    // TODO maybe check if path is actually a file?
    let serialized =
        std::fs::read_to_string(&path).map_err(inerr(errctx("read content to string")))?;
    match get_ext(&path) {
        // TODO remove support for json and add support for bincode
        Some(Ext::JSON) => serde_json::from_str(&serialized)
            .map_err(inerr(errctx("deserialize content from json"))),
        Some(Ext::YAML) => serde_yaml::from_str(&serialized)
            .map_err(inerr(errctx("deserialize content from yaml"))),
        None => Err(unkext(&path)),
    }
}
/// Save a serializable data structure of generic type T to a file, encoding the
/// serialized data based on the extension of the file (see [`Ext`] for the possible
/// extensions)
pub fn save<P: AsRef<Path>, T: Serialize>(path: P, content: &T) -> Result<(), Error> {
    // TODO maybe add an ensure_parent here
    let errctx = error_context(format!("could not save file at path {:?}", path.as_ref()));
    let serialized = match get_ext(&path) {
        // TODO remove support for json and add support for bincode
        Some(Ext::JSON) => {
            serde_json::to_string(content).map_err(inerr(errctx("serialize content to json")))
        }
        Some(Ext::YAML) => {
            serde_yaml::to_string(content).map_err(inerr(errctx("serialize content to yaml")))
        }
        None => Err(unkext(&path)),
    }?;
    std::fs::write(&path, serialized).map_err(inerr(errctx("write content to file")))
}
//--- ---//

//--- LOACAL INFORMATIONS ---//
pub fn home_dir() -> Result<PathBuf, Error> {
    match dirs::home_dir() {
        Some(home_dir) => Ok(home_dir),
        None => {
            return Err(generr(
                "unable to retrieve home directory path",
                "failed to get home directory through crate `dirs`",
            ))
        }
    }
}
pub fn cwd() -> Result<PathBuf, Error> {
    std::env::current_dir().map_err(inerr("failed to retrieve current working directory"))
}
//--- ---//

//--- FS IMPLEMENTATIONS ---//
/// Create a directory if it doesn't exist (creating subpaths recursively if needed)
pub fn create_dir<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let errctx = error_context(format!(
        "could not create directory at path {:?}",
        path.as_ref()
    ));
    if path.as_ref().exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&path).map_err(inerr(errctx("create directory")))
}
/// Ensures that the parent directory of an object exists, creating all the subpath
/// if it doesn't
fn ensure_parent<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let errctx = error_context(format!(
        "could not ensure parent directory at path {:?}",
        path.as_ref()
    ));
    match path.as_ref().parent() {
        Some(parent) if !parent.exists() => {
            create_dir(&parent).map_err(inerr(errctx("create parent")))
        }
        _ => Ok(()),
    }
}
/// Create a file (creating subpaths recursively if needed) and open it in write-only
/// mode
pub fn create_file<P: AsRef<Path>>(path: P) -> Result<std::fs::File, Error> {
    let errctx = error_context(format!("could not create file at path {:?}", path.as_ref()));
    ensure_parent(&path).map_err(inerr(errctx("ensure parent directory")))?;
    std::fs::File::create(&path).map_err(inerr(errctx("create file")))
}
/// Create a file (creating subpaths recursively if needed) and open it in write-only
/// mode, giving an async hangle to the content of the file for asynchronous writing
pub async fn async_create_file<P: AsRef<Path>>(path: P) -> Result<tokio::fs::File, Error> {
    let errctx = error_context(format!(
        "could not async create file at path {:?}",
        path.as_ref()
    ));
    ensure_parent(&path).map_err(inerr(errctx("ensure parent directory")))?;
    tokio::fs::File::create(&path)
        .await
        .map_err(inerr(errctx("create file")))
}
/// Create a symbolic link
pub fn create_symlink<P: AsRef<Path>, T: AsRef<Path>>(path: P, endpoint: T) -> Result<(), Error> {
    let errctx = error_context(format!(
        "could not create symlink at path {:?}",
        path.as_ref()
    ));
    ensure_parent(&path).map_err(inerr(errctx("ensure parent directory")))?;
    std::os::unix::fs::symlink(&endpoint, &path).map_err(inerr(errctx("create symlink")))
}
/// Attempts to list the contents of a directory
pub fn list_dir_content<P: AsRef<Path>>(path: P) -> Result<Vec<PathBuf>, Error> {
    let errmsg = format!("could not list content of dir at path {:?}", path.as_ref());
    let errctx = error_context(errmsg.clone());
    if path.as_ref().get_type() != ObjectType::Dir {
        return Err(wrgobj(
            errmsg + "\nPath is not a dir",
            "object is not a directory",
        ));
    }
    let mut dir_content: Vec<PathBuf> = Vec::new();
    let res = std::fs::read_dir(&path).map_err(inerr(errctx("read dir")))?;
    for entry in res {
        let entry = entry.map_err(inerr(errctx("retrieve value of entry")))?;
        let entry_path = entry.path().to_path_buf();
        dir_content.push(entry_path);
    }

    Ok(dir_content)
}
/// Attempts to open a file in read-only mode
pub fn read_file<P: AsRef<Path>>(path: P) -> Result<std::fs::File, Error> {
    let errctx = error_context(format!("could not open file at path {:?}", path.as_ref()));
    std::fs::File::open(&path).map_err(inerr(errctx("open file")))
}
/// Attempts to open a file in read-only mode, giving an async handle to the content of
/// the file for asynchronous reading
pub async fn async_read_file<P: AsRef<Path>>(path: P) -> Result<tokio::fs::File, Error> {
    let errctx = error_context(format!(
        "could not async open file at path {:?}",
        path.as_ref()
    ));
    tokio::fs::File::open(&path)
        .await
        .map_err(inerr(errctx("open file")))
}
/// Attempts to read the endpoint link of a symlink
pub fn read_link<P: AsRef<Path>>(path: P) -> Result<PathBuf, Error> {
    let errmsg = format!(
        "could not read endpoint of symlink at path {:?}",
        path.as_ref()
    );
    let errctx = error_context(errmsg.clone());
    if path.as_ref().get_type() != ObjectType::SymLink {
        return Err(wrgobj(
            errmsg + "\nPath is not a symlink",
            "object is not a symlink",
        ));
    }
    std::fs::read_link(&path).map_err(inerr(errctx("read link")))
}

/// Attempts to remove a directory, but fails if the directory is not empty
pub fn remove_dir<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let errmsg = format!("could not remove directory at path {:?}", path.as_ref());
    let errctx = error_context(errmsg.clone());
    if path.as_ref().get_type() != ObjectType::Dir {
        return Err(wrgobj(
            errmsg + "\nPath is not a directory",
            "object is not a directory",
        ));
    }
    std::fs::remove_dir(&path).map_err(inerr(errctx("remove directory")))
}
/// Attempts to remove a directory, forcefully removing its content too
pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let errmsg = format!(
        "could not forcefully remove directory at path {:?}",
        path.as_ref()
    );
    let errctx = error_context(errmsg.clone());
    if path.as_ref().get_type() != ObjectType::Dir {
        return Err(wrgobj(
            errmsg + "\nPath is not a directory",
            "object is not a directory",
        ));
    }
    std::fs::remove_dir_all(&path).map_err(inerr(errctx("remove directory")))
}
/// Attempts to remove a file. The inner process is the same as remove_symlink as they
/// both invoke std::fs::remove_file, but it checks that the object at path is a file
/// beforehand
pub fn remove_file<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let errmsg = format!("could not remove file at path {:?}", path.as_ref());
    let errctx = error_context(errmsg.clone());
    if path.as_ref().get_type() != ObjectType::File {
        return Err(wrgobj(
            errmsg + "\nPath is not a file",
            "object is not a file",
        ));
    }
    std::fs::remove_file(&path).map_err(inerr(errctx("remove file")))
}
/// Attempts to remove a symlink. The inner process is the same as remove_file as they
/// both invoke std::fs::remove_file, but it checks that the object at path is a symlink
/// beforehand
pub fn remove_symlink<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let errmsg = format!("could not remove symlink at path {:?}", path.as_ref());
    let errctx = error_context(errmsg.clone());
    if path.as_ref().get_type() != ObjectType::SymLink {
        return Err(wrgobj(
            errmsg + "\nPath is not a symlink",
            "object is not a symlink",
        ));
    }
    std::fs::remove_file(&path).map_err(inerr(errctx("remove file")))
}

/// Attempts to move an object from a specified position to a specified position (does
/// not copy, only attempts to move), creating the necessary subdirectories for the
/// endpoint.<br>
/// Note that this works as expected when acting on a symlink: it renames the symlink at
/// the specified path, preserving the endpoint path (which might not work anymore if it
/// was a relative path), and does NOT move the object pointed by the symlink at the
/// specified path
pub fn rename<P: AsRef<Path>, S: AsRef<Path>>(from: P, to: S) -> Result<(), Error> {
    let errctx = error_context(format!(
        "could not move object from path {:?}, to path {:?}",
        from.as_ref(),
        to.as_ref()
    ));
    ensure_parent(&to).map_err(inerr(errctx("ensure parent directory")))?;
    std::fs::rename(&from, &to).map_err(inerr(errctx("rename object")))
}
/// Ensures that at path there exists an empty directory.
/// It creates the directory if it doesn't exists and it removes all the content
/// if it does
pub fn make_clean_dir<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let errmsg = format!(
        "could not ensure a clean directory at path {:?}",
        path.as_ref()
    );
    let errctx = error_context(errmsg.clone());
    if path.as_ref().exists() {
        if path.as_ref().get_type() != ObjectType::Dir {
            return Err(wrgobj(
                errmsg + "\nPath is not a directory",
                "object is not a directory",
            ));
        }

        remove_dir_all(&path).map_err(inerr(errctx("remove all the content")))?;
    }
    create_dir(&path).map_err(inerr(errctx("create directory")))
}
//--- ---//

//--- METADATA ---//
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
/// Structure containing (some of) the metadata of a either a directory or a file.
pub struct Metadata {
    mtime: (i64, u32),
    mode: u32,
}

impl Metadata {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.append(&mut self.mtime.0.to_be_bytes().to_vec());
        bytes.append(&mut self.mtime.1.to_be_bytes().to_vec());
        bytes.append(&mut self.mode.to_be_bytes().to_vec());

        bytes
    }

    pub fn to_oct(&self) -> String {
        let output = format!("{:o}", self.mode);
        output[output.len() - 3..].to_string()
    }

    pub fn format(&self) -> String {
        let perms = format!("{:o}", self.mode);
        let perms = perms[perms.len() - 3..].to_string();
        format!(
            "{} {}",
            perms,
            chrono::NaiveDateTime::from_timestamp(self.mtime.0, self.mtime.1)
        )
    }
}

fn time_to_pair(time: FileTime) -> (i64, u32) {
    (time.unix_seconds(), time.nanoseconds())
}
fn pair_to_time((sec, nano): (i64, u32)) -> FileTime {
    filetime::FileTime::from_unix_time(sec, nano)
}

/// Set metadata of an object (directory or file)
///
/// Returns an error if the std::fs fails to retrieve metadata
/// from the specified path, in which case it returns the
/// wrapped error
pub fn get_metadata<T: AsRef<Path>>(path: T) -> Result<Metadata, Error> {
    let errctx = error_context(format!(
        "could not get metadata from path {:?}",
        path.as_ref()
    ));
    let os_metadata = std::fs::metadata(&path).map_err(inerr(errctx("get metadata of object")))?;
    let mtime = filetime::FileTime::from_last_modification_time(&os_metadata);
    let mode = os_metadata.mode();

    Ok(Metadata {
        mtime: time_to_pair(mtime),
        mode,
    })
}

/// Set metadata of an object (directory or file)
///
/// Returns an error if:
/// - std::fs fails to retrieve metadata from the specified path
/// - filetime fails to set times
/// - std::fs fails to set permissions
/// in any of these cases it returns the wrapped error
pub fn set_metadata<T: AsRef<Path>>(path: T, metadata: &Metadata) -> Result<(), Error> {
    let errctx = error_context(format!(
        "could not set metadata from path {:?}",
        path.as_ref()
    ));
    let mtime = pair_to_time(metadata.mtime);
    let mut perms = std::fs::metadata(&path)
        .map_err(inerr(errctx("retrieve old metadata of object")))?
        .permissions();
    perms.set_mode(metadata.mode);
    std::fs::set_permissions(&path, perms).map_err(inerr(errctx("set permissions")))?;
    filetime::set_file_mtime(&path, mtime).map_err(inerr(errctx("set modification time")))?;

    Ok(())
}
//--- ---//
