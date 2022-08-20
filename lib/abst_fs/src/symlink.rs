use std::io::{BufRead, Write};

use super::{
    ensure_parent, error_context, generr, inerr, wrgobj, AbstPath, Endpoint, Error, ObjectType,
};

pub const ABST_OBJ_HEADER: &str = "[[bbup abstract symlink object]]";

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

/// Attempts to read the endpoint link of a symlink
pub fn read_link(path: &AbstPath) -> Result<Endpoint, Error> {
    use super::path::ForceToString;

    let errmsg = format!("could not read endpoint of symlink at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::SymLink) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nSymLink doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a symlink",
                "object is not a symlink",
            ));
        }
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

/// Attempts to remove a symlink. The inner process is the same as remove_file as they
/// both invoke std::fs::remove_file, but it checks that the object at path is a symlink
/// beforehand
pub fn remove_symlink(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not remove symlink at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::SymLink) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nSymLink doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a symlink",
                "object is not a symlink",
            ));
        }
    }
    std::fs::remove_file(path.to_path_buf()).map_err(inerr(errctx("remove file")))
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
    match from.object_type() {
        Some(ObjectType::SymLink) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nSymLink doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a symlink",
                "object is not a symlink",
            ));
        }
    }
    ensure_parent(to).map_err(inerr(errctx("ensure parent directory")))?;
    std::fs::rename(from.to_path_buf(), to.to_path_buf()).map_err(inerr(errctx("rename object")))
}

fn trim_newline(s: &mut String) {
    if s.ends_with('\n') {
        s.pop();
        if s.ends_with('\r') {
            s.pop();
        }
    }
}
