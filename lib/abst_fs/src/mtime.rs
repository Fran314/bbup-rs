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

    #[test]
    fn test() {
        from();
        to_bytes();
        to_string();
        get_set_mtime();
    }

    fn from() {
        assert_eq!(TEST_MTIME, Mtime::from(498705663, 141592653));
        assert_eq!(
            Mtime(1132648943, 735182781),
            Mtime::from(1132648943, 735182781)
        );
    }
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

    fn to_string() {
        assert_eq!(format!("{TEST_MTIME}"), TEST_MTIME_STRING);
    }

    fn get_set_mtime() {
        let path_bf = PathBuf::from("/tmp/bbup-test-abst_fs-mtime");
        let path = (AbstPath::from(&path_bf), path_bf);
        //	make sure the path means actually what I think it mean
        assert_eq!(path.0.to_path_buf(), path.1);
        assert!(!path.1.exists());

        std::fs::create_dir(&path.1).unwrap();

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

        std::fs::remove_dir_all(&path.1).unwrap();

        assert!(result.is_ok())
    }
}
