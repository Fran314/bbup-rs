use std::io::{BufRead, Write};

use super::{
    error_context, generr, inerr, wrgobj, AbstPath, Endpoint, Error, ObjectType, ABST_OBJ_HEADER,
};

pub trait ForceToString {
    fn force_to_string(&self) -> String;
}
impl ForceToString for std::ffi::OsStr {
    fn force_to_string(&self) -> String {
        self.to_str()
            .unwrap_or_else(|| {
                panic!(
                    "Broken path: could not convert from os string to valid utf8\nos string: {:?}",
                    self
                )
            })
            .to_string()
    }
}
impl ForceToString for std::path::Path {
    fn force_to_string(&self) -> String {
        self.as_os_str().force_to_string()
    }
}
impl ForceToString for std::path::PathBuf {
    fn force_to_string(&self) -> String {
        self.as_os_str().force_to_string()
    }
}
//--- ---//

/// Create a directory if it doesn't exist (creating subpaths recursively if needed)
pub fn create_dir(path: &AbstPath) -> Result<(), Error> {
    let errctx = error_context(format!("could not create directory at path {path}"));
    match path.object_type() {
        Some(ObjectType::Dir) => Ok(()),
        None => {
            std::fs::create_dir_all(path.to_path_buf()).map_err(inerr(errctx("create directory")))
        }
        _ => Err(generr(
            errctx("create directory"),
            "objet already exists but is not a directory",
        )),
    }
}
/// Ensures that the parent directory of an object exists, creating all the subpath
/// if it doesn't
pub fn ensure_parent(path: &AbstPath) -> Result<(), Error> {
    let errctx = error_context(format!(
        "could not ensure parent directory at path {}",
        path
    ));
    match path.parent() {
        Some(parent) if !parent.exists() => {
            create_dir(&parent).map_err(inerr(errctx("create parent")))
        }
        _ => Ok(()),
    }
}
/// Create a file (creating subpaths recursively if needed) and open it in write-only
/// mode
pub fn create_file(path: &AbstPath) -> Result<std::fs::File, Error> {
    let errctx = error_context(format!("could not create file at path {path}"));
    ensure_parent(path).map_err(inerr(errctx("ensure parent directory")))?;
    std::fs::File::create(path.to_path_buf()).map_err(inerr(errctx("create file")))
}
/// Create a file (creating subpaths recursively if needed) and open it in write-only
/// mode, giving an async hangle to the content of the file for asynchronous writing
pub async fn async_create_file(path: &AbstPath) -> Result<tokio::fs::File, Error> {
    let errctx = error_context(format!("could not async create file at path {path}"));
    ensure_parent(path).map_err(inerr(errctx("ensure parent directory")))?;
    tokio::fs::File::create(path.to_path_buf())
        .await
        .map_err(inerr(errctx("create file")))
}

/// Create a symbolic link
pub fn create_symlink(path: &AbstPath, endpoint: Endpoint) -> Result<(), Error> {
    let errctx = error_context(format!("could not create symlink at path {path}"));
    ensure_parent(path).map_err(inerr(errctx("ensure parent directory")))?;

    #[cfg(unix)]
    match endpoint {
        Endpoint::Unix(endpath) => {
            std::os::unix::fs::symlink(&endpath, path.to_path_buf())
                .map_err(inerr(errctx("create unix symlink")))?;
        }

        Endpoint::Windows(is_dir, endpath) => {
            let mut abstract_symlink = std::fs::File::create(path.to_path_buf())
                .map_err(inerr(errctx("create abstract symlink")))?;
            let endpoint_type = if is_dir { "dir" } else { "file" };
            abstract_symlink
                .write(format!("{ABST_OBJ_HEADER}\nwindows\n{endpoint_type}\n{endpath}").as_bytes())
                .map_err(inerr(errctx("write abstract symlink content")))?;
        }
    }

    #[cfg(windows)]
    match endpoint {
        Endpoint::Windows(is_dir, endpath) => {
            match is_dir {
                true => std::os::windows::fs::symlink_dir(&endpath, path.to_path_buf())
                    .map_err(inerr(errctx("create windows dir symlink")))?,
                false => std::os::windows::fs::symlink_file(&endpath, path.to_path_buf())
                    .map_err(inerr(errctx("create windows file symlink")))?,
            };
        }
        Endpoint::Unix(endpath) => {
            let mut abstract_symlink = std::fs::File::create(path.to_path_buf())
                .map_err(inerr(errctx("create abstract symlink")))?;
            abstract_symlink
                .write(format!("{ABST_OBJ_HEADER}\nunix\n{endpath}").as_bytes())
                .map_err(inerr(errctx("write abstract symlink content")))?;
        }
    }

    Ok(())
}
/// Attempts to list the contents of a directory
pub fn list_dir_content(path: &AbstPath) -> Result<Vec<AbstPath>, Error> {
    let errmsg = format!("could not list content of dir at path {path}");
    let errctx = error_context(errmsg.clone());
    if path.object_type() != Some(ObjectType::Dir) {
        return Err(wrgobj(
            errmsg + "\nPath is not a dir",
            "object is not a directory",
        ));
    }
    let mut dir_content: Vec<AbstPath> = Vec::new();
    let res = std::fs::read_dir(path.to_path_buf()).map_err(inerr(errctx("read dir")))?;
    for entry in res {
        let entry = entry.map_err(inerr(errctx("retrieve value of entry")))?;
        let entry_path = entry.path().to_path_buf();
        dir_content.push(AbstPath::from(entry_path));
    }

    Ok(dir_content)
}
/// Attempts to open a file in read-only mode
pub fn read_file(path: &AbstPath) -> Result<std::fs::File, Error> {
    let errctx = error_context(format!("could not open file at path {path}"));
    std::fs::File::open(path.to_path_buf()).map_err(inerr(errctx("open file")))
}
/// Attempts to open a file in read-only mode, giving an async handle to the content of
/// the file for asynchronous reading
pub async fn async_read_file(path: &AbstPath) -> Result<tokio::fs::File, Error> {
    let errctx = error_context(format!("could not async open file at path {path}"));
    tokio::fs::File::open(path.to_path_buf())
        .await
        .map_err(inerr(errctx("open file")))
}
/// Attempts to read the endpoint link of a symlink
pub fn read_link(path: &AbstPath) -> Result<Endpoint, Error> {
    let errmsg = format!("could not read endpoint of symlink at path {path}");
    let errctx = error_context(errmsg.clone());
    if path.object_type() != Some(ObjectType::SymLink) {
        return Err(wrgobj(
            errmsg + "\nPath is not a symlink",
            "object is not a symlink",
        ));
    }
    let metadata = std::fs::symlink_metadata(path.to_path_buf()).map_err(inerr(errctx(
        "get metadata to establish if actual symlink or abstract symlink",
    )))?;
    match metadata.is_symlink() {
        #[cfg(unix)]
        true => {
            let endpath = std::fs::read_link(path.to_path_buf())
                .map_err(inerr(errctx("read endpoint of actual symlink")))?;
            Ok(Endpoint::Unix(endpath.force_to_string()))
        }
        #[cfg(windows)]
        true => {
            // TODO remove this * and add only the necessary stuff please
            use std::os::windows::prelude::*;
            let endpath = std::fs::read_link(path.to_path_buf())
                .map_err(inerr(errctx("read endpoint of actual symlink")))?;

            let is_dir = (metadata.file_attributes() & 16) == 16;
            Ok(Endpoint::Windows(is_dir, endpath.force_to_string()))
        }
        false => {
            let abstract_symlink = std::fs::File::open(path.to_path_buf())
                .map_err(inerr(errctx("open abstract symlink")))?;
            let mut reader = std::io::BufReader::new(abstract_symlink);

            // Skip header: we already know this is an ObjectTYpe::SymLink and not
            //	an actual symlink so it must be an abstract symlink and we know that
            //	it has the correct header, so no need to check
            reader
                .read_line(&mut String::new())
                .map_err(inerr(errctx("read abstract symlink header")))?;

            let mut os = String::new();
            reader
                .read_line(&mut os)
                .map_err(inerr(errctx("read abstract symlink os")))?;
            trim_newline(&mut os);
            match os.as_str() {
                "unix" => {
                    let mut endpath = String::new();
                    reader
                        .read_line(&mut endpath)
                        .map_err(inerr(errctx("read abstract symlink endpath")))?;
                    Ok(Endpoint::Unix(endpath))
                }
                "windows" => {
                    let mut endpoint_type = String::new();
                    reader
                        .read_line(&mut endpoint_type)
                        .map_err(inerr(errctx("read abstract symlink endpath")))?;
                    trim_newline(&mut endpoint_type);
                    let is_dir = match endpoint_type.as_str() {
                        "dir" => true,
                        "file" => false,
                        val => {
                            return Err(generr(
                                errmsg + "\nInvalid endpoint type",
                                format!("{val:?} is not a valid endpoint type"),
                            ));
                        }
                    };
                    let mut endpath = String::new();
                    reader
                        .read_line(&mut endpath)
                        .map_err(inerr(errctx("read abstract symlink endpath")))?;
                    Ok(Endpoint::Windows(is_dir, endpath))
                }
                val => Err(generr(
                    errmsg + "\nInvalid os",
                    format!("{val:?} is not a valid os"),
                )),
            }
        }
    }
}

/// Attempts to remove a directory, but fails if the directory is not empty
pub fn remove_dir(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not remove directory at path {path}");
    let errctx = error_context(errmsg.clone());
    if path.object_type() != Some(ObjectType::Dir) {
        return Err(wrgobj(
            errmsg + "\nPath is not a directory",
            "object is not a directory",
        ));
    }
    std::fs::remove_dir(path.to_path_buf()).map_err(inerr(errctx("remove directory")))
}
/// Attempts to remove a directory, forcefully removing its content too
pub fn remove_dir_all(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not forcefully remove directory at path {path}");
    let errctx = error_context(errmsg.clone());
    if path.object_type() != Some(ObjectType::Dir) {
        return Err(wrgobj(
            errmsg + "\nPath is not a directory",
            "object is not a directory",
        ));
    }
    std::fs::remove_dir_all(path.to_path_buf()).map_err(inerr(errctx("remove directory")))
}
/// Attempts to remove a file. The inner process is the same as remove_symlink as they
/// both invoke std::fs::remove_file, but it checks that the object at path is a file
/// beforehand
pub fn remove_file(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not remove file at path {path}");
    let errctx = error_context(errmsg.clone());
    if path.object_type() != Some(ObjectType::File) {
        return Err(wrgobj(
            errmsg + "\nPath is not a file",
            "object is not a file",
        ));
    }
    std::fs::remove_file(path.to_path_buf()).map_err(inerr(errctx("remove file")))
}
/// Attempts to remove a symlink. The inner process is the same as remove_file as they
/// both invoke std::fs::remove_file, but it checks that the object at path is a symlink
/// beforehand
pub fn remove_symlink(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not remove symlink at path {path}");
    let errctx = error_context(errmsg.clone());
    if path.object_type() != Some(ObjectType::SymLink) {
        return Err(wrgobj(
            errmsg + "\nPath is not a symlink",
            "object is not a symlink",
        ));
    }
    std::fs::remove_file(path.to_path_buf()).map_err(inerr(errctx("remove file")))
}

/// Attempts to move a file from a specified position to a specified position (does
/// not copy, only attempts to move), creating the necessary subdirectories for the
/// endpoint
pub fn rename_file(from: &AbstPath, to: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not move object from path {from}, to path {to}");
    let errctx = error_context(errmsg.clone());
    if from.object_type() != Some(ObjectType::File) {
        return Err(wrgobj(
            errmsg + "\nPath is not a file",
            "object is not a file",
        ));
    }
    ensure_parent(to).map_err(inerr(errctx("ensure parent directory")))?;
    std::fs::rename(from.to_path_buf(), to.to_path_buf()).map_err(inerr(errctx("rename object")))
}

/// Attempts to move a symlink from a specified position to a specified position (does
/// not copy, only attempts to move), creating the necessary subdirectories if
/// needed.<br>
/// Note that this works as expected on the symlink's endpoint: it renames the symlink at
/// the specified path, preserving the endpoint path (which might not work anymore if it
/// was a relative path), and does NOT move the object pointed by the symlink at the
/// specified path
pub fn rename_symlink(from: &AbstPath, to: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not move object from path {from}, to path {to}");
    let errctx = error_context(errmsg.clone());
    if from.object_type() != Some(ObjectType::SymLink) {
        return Err(wrgobj(
            errmsg + "\nPath is not a symlink",
            "object is not a symlink",
        ));
    }
    ensure_parent(to).map_err(inerr(errctx("ensure parent directory")))?;
    std::fs::rename(from.to_path_buf(), to.to_path_buf()).map_err(inerr(errctx("rename object")))
}
/// Ensures that at path there exists an empty directory.
/// It creates the directory if it doesn't exists and it removes all the content
/// if it does
pub fn make_clean_dir(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not ensure a clean directory at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        None => { /* ... */ }
        Some(ObjectType::Dir) => {
            remove_dir_all(path).map_err(inerr(errctx("remove all the content")))?
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a directory",
                "object is not a directory",
            ));
        }
    }
    create_dir(path).map_err(inerr(errctx("create directory")))
}

//--- LOCAL UTILITY FUNCTIONS ---//
fn trim_newline(s: &mut String) {
    if s.ends_with('\n') {
        s.pop();
        if s.ends_with('\r') {
            s.pop();
        }
    }
}

//--- ---//
