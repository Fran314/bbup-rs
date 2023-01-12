use serde::{Deserialize, Serialize};

use super::{error_context, inerr, AbstPath, Error};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Mtime(i64, u32);

impl Mtime {
    pub fn from(time: i64, nanoseconds: u32) -> Mtime {
        Mtime(time, nanoseconds)
    }
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
    use super::{get_mtime, set_mtime, AbstPath, Mtime};
    use std::path::PathBuf;

    const TEST_MTIME: Mtime = Mtime(498705663, 141592653);
    const TEST_MTIME_STRING: &str = "1985-10-21 01:21:03.141592653";

    // --- SAFETY MEASURES --- //
    // Testing this crate is dangerous as an unexpected behaviour could
    // potentially damage the machine it's ran on. To fix this we uso two safety
    // measures: we create object and work only inside the `/tmp` folder, and
    // we make sure that the paths (AbstPath) that we use actually mean what we
    // think they mean, to avoid situations where we think we're deleting a file
    // inside /tmp but actually we're deleting a home directory.
    //
    // The purpose of `safe_add_last` is to append components to paths while
    // checking that they still mean what we think they mean.
    //
    // `setup_sandbox` and `cleanup_sandbox` create and destroy the sandbox
    // environments in which the testing will happen. The creation happens only
    // after having checked that the path didn't get corrupted while being
    // transformed into an AbstPath
    trait SafeAdd {
        fn safe_add_last<S: std::string::ToString>(&self, suffix: S) -> (AbstPath, PathBuf);
    }
    impl SafeAdd for (AbstPath, PathBuf) {
        fn safe_add_last<S: std::string::ToString>(&self, suffix: S) -> (AbstPath, PathBuf) {
            let (abst, pb) = self;
            let new_abst = abst.add_last(suffix.to_string());
            let new_pb = pb.join(suffix.to_string());
            //	make sure the path means actually what I think it mean
            assert_eq!(new_abst.to_path_buf(), new_pb);
            (new_abst, new_pb)
        }
    }
    fn setup_sandbox(path: impl std::fmt::Display) -> (AbstPath, PathBuf) {
        let path_bf = PathBuf::from(format!("/tmp/{path}"));
        assert!(!path_bf.exists());
        std::fs::create_dir(&path_bf).unwrap();

        let path = (AbstPath::from(&path_bf), path_bf);
        assert_eq!(path.0.to_path_buf(), path.1); // make sure the path means actually what I think it mean
        path
    }
    fn cleanup_sandbox(path: impl std::fmt::Display) {
        let path_bf = PathBuf::from(format!("/tmp/{path}"));
        std::fs::remove_dir_all(&path_bf).unwrap();
    }
    // --- --- //

    #[test]
    fn from() {
        assert_eq!(TEST_MTIME, Mtime::from(498705663, 141592653));
        assert_eq!(
            Mtime(1132648943, 735182781),
            Mtime::from(1132648943, 735182781)
        );
    }
    #[test]
    fn to_bytes() {
        assert_ne!(
            TEST_MTIME.to_bytes(),
            Mtime(1132648943, 141592653).to_bytes()
        );
        assert_ne!(
            TEST_MTIME.to_bytes(),
            Mtime(498705663, 735182781).to_bytes()
        );
        assert_ne!(
            TEST_MTIME.to_bytes(),
            Mtime(1132648943, 735182781).to_bytes()
        );
    }

    #[test]
    fn to_string() {
        assert_eq!(format!("{TEST_MTIME}"), TEST_MTIME_STRING);
    }

    #[test]
    // While it is not ideal to have one huge test function testing all the
    // possible behaviours, given the possibility of danger of these tests it is
    // better to execute them sequencially in a deliberate order rather than
    // in parallel or in random order. This is why the tests for this module are
    // all in one huge function
    fn get_set_mtime() {
        #[cfg(not(unix))]
        panic!("this test is meant to be ran on a Unix system");

        let sandbox = "bbup-test-abst_fs-mtime";
        let path = setup_sandbox(sandbox);

        let result = std::panic::catch_unwind(|| {
            let (dir, _) = path.safe_add_last("dir");
            std::fs::create_dir(dir.to_path_buf()).unwrap();
            get_mtime(&dir).unwrap();
            set_mtime(&dir, &TEST_MTIME).unwrap();
            assert_eq!(get_mtime(&dir).unwrap(), TEST_MTIME);

            let (symlink, _) = path.safe_add_last("symlink");
            std::os::unix::fs::symlink("some/path/to/somewhere", symlink.to_path_buf()).unwrap();
            get_mtime(&symlink).unwrap();
            set_mtime(&symlink, &TEST_MTIME).unwrap();
            assert_eq!(get_mtime(&symlink).unwrap(), TEST_MTIME);

            let (file, _) = path.safe_add_last("file");
            std::fs::File::create(file.to_path_buf()).unwrap();
            get_mtime(&file).unwrap();
            set_mtime(&file, &TEST_MTIME).unwrap();
            assert_eq!(get_mtime(&file).unwrap(), TEST_MTIME);

            let (non_existing_object, _) = path.safe_add_last("non_existing_object");
            assert!(get_mtime(&non_existing_object).is_err());
        });

        cleanup_sandbox(sandbox);

        assert!(result.is_ok())
    }
}
