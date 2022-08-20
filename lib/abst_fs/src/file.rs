use super::{ensure_parent, error_context, inerr, wrgobj, AbstPath, Error, ObjectType};

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

/// Attempts to open a file in read-only mode
pub fn read_file(path: &AbstPath) -> Result<std::fs::File, Error> {
    let errmsg = format!("could not open file at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::File) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nFile doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a file",
                "object is not a file",
            ));
        }
    }
    std::fs::File::open(path.to_path_buf()).map_err(inerr(errctx("open file")))
}
/// Attempts to open a file in read-only mode, giving an async handle to the content of
/// the file for asynchronous reading
pub async fn async_read_file(path: &AbstPath) -> Result<tokio::fs::File, Error> {
    let errmsg = format!("could not async open file at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::File) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nFile doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a file",
                "object is not a file",
            ));
        }
    }
    tokio::fs::File::open(path.to_path_buf())
        .await
        .map_err(inerr(errctx("open file")))
}

/// Attempts to remove a file. The inner process is the same as remove_symlink as they
/// both invoke std::fs::remove_file, but it checks that the object at path is a file
/// beforehand
pub fn remove_file(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not remove file at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::File) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nFile doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a file",
                "object is not a file",
            ));
        }
    }
    std::fs::remove_file(path.to_path_buf()).map_err(inerr(errctx("remove file")))
}

/// Attempts to move a file from a specified position to a specified position (does
/// not copy, only attempts to move), creating the necessary subdirectories for the
/// endpoint
pub fn rename_file(from: &AbstPath, to: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not move object from path {from}, to path {to}");
    let errctx = error_context(errmsg.clone());
    match from.object_type() {
        Some(ObjectType::File) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nFile doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a file",
                "object is not a file",
            ));
        }
    }
    ensure_parent(to).map_err(inerr(errctx("ensure parent directory")))?;
    std::fs::rename(from.to_path_buf(), to.to_path_buf()).map_err(inerr(errctx("rename object")))
}

#[cfg(test)]
mod tests {
    use super::{
        async_create_file, async_read_file, create_file, read_file, remove_file, rename_file,
        AbstPath,
    };
    use std::path::PathBuf;

    #[test]
    fn test() {
        use std::io::{BufReader, Read, Write};

        let path_bf = PathBuf::from("/tmp/bbup-test-abst_fs-file");
        let path = AbstPath::from(&path_bf);
        //	make sure paths mean actually what I think they mean
        assert_eq!(path.to_path_buf(), path_bf);

        if path_bf.exists() {
            panic!(
                "path [{path_bf:?}] should not exist in order to run this test, but it does exist!"
            );
        }

        let result = std::panic::catch_unwind(|| {
            std::fs::create_dir(&path_bf).unwrap();

            // create_file
            let file = path.add_last("file.txt");
            assert_eq!(file.to_path_buf(), path_bf.join("file.txt"));
            assert!(!file.exists());
            create_file(&file).unwrap();
            assert!(file.exists());

            // read_file
            let mut reader = BufReader::new(read_file(&file).unwrap());
            let mut buffer = String::new();
            assert_eq!(reader.read_to_string(&mut buffer).unwrap(), 0);
            assert_eq!(buffer, String::new());

            let mut writer = create_file(&file).unwrap();
            let dummy_content = "here is some dummy text";
            writer.write_all(dummy_content.as_bytes()).unwrap();

            let mut reader = BufReader::new(read_file(&file).unwrap());
            let mut buffer = String::new();
            reader.read_to_string(&mut buffer).unwrap();
            assert_eq!(buffer, String::from(dummy_content));

            let dir = path.add_last("dir");
            assert_eq!(dir.to_path_buf(), path_bf.join("dir"));
            std::fs::create_dir(dir.to_path_buf()).unwrap();
            assert!(read_file(&dir).is_err());

            // rename_file
            let file2 = path.add_last("file2");
            assert_eq!(file2.to_path_buf(), path_bf.join("file2"));
            rename_file(&file, &file2).unwrap();
            rename_file(&file2, &file).unwrap();

            let non_existing_file = path.add_last("file-that-doesn't-exist.txt");
            assert_eq!(
                non_existing_file.to_path_buf(),
                path_bf.join("file-that-doesn't-exist.txt")
            );
            assert!(rename_file(&non_existing_file, &file2).is_err());

            let symlink = path.add_last("symlink.ln");
            assert_eq!(symlink.to_path_buf(), path_bf.join("symlink.ln"));
            std::os::unix::fs::symlink(".", symlink.to_path_buf()).unwrap();
            assert!(rename_file(&symlink, &file2).is_err());

            // remove_file
            remove_file(&file).unwrap();
            assert!(remove_file(&symlink).is_err());
        });

        if path_bf.exists() {
            std::fs::remove_dir_all(path_bf).unwrap();
        }

        assert!(result.is_ok())
    }

    #[tokio::test]
    async fn async_test() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

        let path_bf = PathBuf::from("/tmp/bbup-test-abst_fs-file-async");
        let path_bf_clone = path_bf.clone();
        let path = AbstPath::from(&path_bf);
        //	make sure paths mean actually what I think they mean
        assert_eq!(path.to_path_buf(), path_bf);

        if path_bf.exists() {
            panic!(
                "path [{path_bf:?}] should not exist in order to run this test, but it does exist!"
            );
        }

        let result = std::panic::catch_unwind(|| async {
            std::fs::create_dir(&path_bf).unwrap();

            // async_create_file
            let file = path.add_last("file.txt");
            assert_eq!(file.to_path_buf(), path_bf.join("file.txt"));
            assert!(!file.exists());
            async_create_file(&file).await.unwrap();
            assert!(file.exists());

            // async_read_file
            let mut reader = BufReader::new(async_read_file(&file).await.unwrap());
            let mut buffer = String::new();
            assert_eq!(reader.read_to_string(&mut buffer).await.unwrap(), 0);
            assert_eq!(buffer, String::new());

            let mut writer = async_create_file(&file).await.unwrap();
            let dummy_content = "here is some dummy text";
            writer.write_all(dummy_content.as_bytes()).await.unwrap();

            let mut reader = BufReader::new(async_read_file(&file).await.unwrap());
            let mut buffer = String::new();
            reader.read_to_string(&mut buffer).await.unwrap();
            assert_eq!(buffer, String::from(dummy_content));

            let dir = path.add_last("dir");
            assert_eq!(dir.to_path_buf(), path_bf.join("dir"));
            std::fs::create_dir(dir.to_path_buf()).unwrap();
            assert!(async_read_file(&dir).await.is_err());
        });

        if path_bf_clone.exists() {
            std::fs::remove_dir_all(path_bf_clone).unwrap();
        }

        assert!(result.is_ok())
    }
}
