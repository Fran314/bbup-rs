use serde::{Deserialize, Serialize};

use super::{error_context, inerr, AbstPath, Error};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Mtime(i64, u32);

impl Mtime {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.append(&mut self.0.to_be_bytes().to_vec());
        bytes.append(&mut self.1.to_be_bytes().to_vec());

        bytes
    }
}

impl std::fmt::Display for Mtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let timestamp = chrono::NaiveDateTime::from_timestamp(self.0, self.1);
        write!(f, "{}", timestamp)
    }
}

/// Get mtime of an object
///
/// Returns an error if the std::fs fails to retrieve metadata
/// from the specified path, in which case it returns the
/// wrapped error
pub fn get_mtime(path: &AbstPath) -> Result<Mtime, Error> {
    let errctx = error_context(format!("could not get mtime from path {path}"));
    let metadata = std::fs::symlink_metadata(path.to_path_buf())
        .map_err(inerr(errctx("get metadata of object")))?;
    let mtime = filetime::FileTime::from_last_modification_time(&metadata);

    Ok(Mtime(mtime.unix_seconds(), mtime.nanoseconds()))
}

/// Set mtime of an object
///
/// NOTE: This function will actually set both atime and mtime to the time
/// specified. This is because the crate filetime does not provide (yet)
/// separate functions to set either the atime or the mtime if we don't want to
/// follow symlinks, which this utility does not.
///
/// Returns an error if:
/// - std::fs fails to retrieve metadata from the specified path
/// - filetime fails to set times
/// - std::fs fails to set permissions
/// in any of these cases it returns the wrapped error
pub fn set_mtime(path: &AbstPath, mtime: &Mtime) -> Result<(), Error> {
    let errctx = error_context(format!("could not set mtime at path {path}"));
    let mtime = filetime::FileTime::from_unix_time(mtime.0, mtime.1);

    // Sets BOTH atime and mtime to the specified mtime. Not optimal but easiest
    //	solution if we don't want to follow symlinks
    filetime::set_symlink_file_times(path.to_path_buf(), mtime, mtime)
        .map_err(inerr(errctx("set mime")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Mtime;

    #[test]
    fn to_string() {
        let mtime = Mtime(498705663, 141592653);
        assert_eq!(format!("{mtime}"), "1985-10-21 01:21:03.141592653");
    }

    // #[test]
    // fn get_set_mtime() {
    //     unimplemented!("I first have to figure out how to test file system stuff in a safe way");
    // }
}
