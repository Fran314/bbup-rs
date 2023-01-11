use super::{error_context, generr, inerr, wrgobj, AbstPath, Error, ObjectType};

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

/// Attempts to list the contents of a directory
pub fn list_dir_content(path: &AbstPath) -> Result<Vec<AbstPath>, Error> {
    let errmsg = format!("could not list content of dir at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::Dir) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nDirectory doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a dir",
                "object is not a directory",
            ));
        }
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

/// Attempts to remove a directory, but fails if the directory is not empty
pub fn remove_dir(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not remove directory at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::Dir) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nDirectory doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a dir",
                "object is not a directory",
            ));
        }
    }
    std::fs::remove_dir(path.to_path_buf()).map_err(inerr(errctx("remove directory")))
}
/// Attempts to remove a directory, forcefully removing its content too
pub fn remove_dir_all(path: &AbstPath) -> Result<(), Error> {
    let errmsg = format!("could not forcefully remove directory at path {path}");
    let errctx = error_context(errmsg.clone());
    match path.object_type() {
        Some(ObjectType::Dir) => { /* ... */ }
        None => {
            return Err(wrgobj(
                errmsg + "\nDirectory doesn't exist",
                "object doesn't exist",
            ));
        }
        _ => {
            return Err(wrgobj(
                errmsg + "\nPath is not a dir",
                "object is not a directory",
            ));
        }
    }
    std::fs::remove_dir_all(path.to_path_buf()).map_err(inerr(errctx("remove directory")))
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

#[cfg(test)]
mod tests {
    use super::{
        create_dir, ensure_parent, list_dir_content, make_clean_dir, remove_dir, remove_dir_all,
        AbstPath, ObjectType,
    };
    use std::path::PathBuf;

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
    // While it is not ideal to have one huge test function testing all the
    // possible behaviours, given the possibility of danger of these tests it is
    // better to execute them sequencially in a deliberate order rather than
    // in parallel or in random order. This is why the tests for this module are
    // all in one huge function
    fn test() {
        let sandbox = "bbup-test-abst_fs-directory";
        let path = setup_sandbox(sandbox);

        let result = std::panic::catch_unwind(|| {
            // create_dir
            let dir = path.safe_add_last("dir");
            assert!(!dir.0.exists());
            create_dir(&dir.0).expect("could not create dir");
            create_dir(&dir.0).expect("failed to do nothing on creating dir that already exists");
            assert!(dir.0.exists());
            assert_eq!(dir.0.object_type(), Some(ObjectType::Dir));
            let file = dir.safe_add_last("file.txt");
            std::fs::File::create(&file.1).unwrap();
            assert!(create_dir(&file.0).is_err());
            std::fs::remove_file(&file.1).unwrap();

            // list_dir_content
            assert_eq!(list_dir_content(&dir.0).unwrap(), vec![]);

            let (non_existing_dir, _) = dir.safe_add_last("non-existing-dir");
            assert!(list_dir_content(&non_existing_dir).is_err());
            let (file1, _) = dir.safe_add_last("file1.txt");
            let (file2, _) = dir.safe_add_last("file2.png");
            let (symlink1, _) = dir.safe_add_last("symlink1.ln");
            let (symlink2, _) = dir.safe_add_last("symlink2");
            let (dir1, _) = dir.safe_add_last("dir1");
            let (dir2, _) = dir.safe_add_last("dir2");
            std::fs::File::create(file1.to_path_buf()).unwrap();
            assert!(list_dir_content(&file1).is_err());
            std::fs::File::create(file2.to_path_buf()).unwrap();
            std::os::unix::fs::symlink(".", symlink1.to_path_buf()).unwrap();
            std::os::unix::fs::symlink(".", symlink2.to_path_buf()).unwrap();
            create_dir(&dir1).unwrap();
            create_dir(&dir2).unwrap();
            let mut dir_list = list_dir_content(&dir.0)
                .unwrap()
                .into_iter()
                .map(|path| path.to_string())
                .collect::<Vec<String>>();
            dir_list.sort();
            let mut artificialdir_list = vec![file1.clone(), file2, symlink1, symlink2, dir1, dir2]
                .into_iter()
                .map(|path| path.to_string())
                .collect::<Vec<String>>();
            artificialdir_list.sort();
            assert_eq!(dir_list, artificialdir_list);

            // make_clean_dir
            assert!(dir.0.exists());
            assert_ne!(list_dir_content(&dir.0).unwrap(), vec![]);

            assert!(make_clean_dir(&file1).is_err());
            make_clean_dir(&dir.0).unwrap();
            assert!(dir.0.exists());
            assert_eq!(list_dir_content(&dir.0).unwrap(), vec![]);

            make_clean_dir(&dir.0).unwrap();
            assert!(dir.0.exists());
            assert_eq!(list_dir_content(&dir.0).unwrap(), vec![]);

            remove_dir(&dir.0).unwrap();
            assert!(!dir.0.exists());

            make_clean_dir(&dir.0).unwrap();
            assert!(dir.0.exists());
            assert_eq!(list_dir_content(&dir.0).unwrap(), vec![]);

            // ensure_parent
            let parent = dir.safe_add_last("test").safe_add_last("something");
            let (child, _) = parent.safe_add_last("file.txt");
            let parent = parent.0;
            assert!(!parent.exists());
            ensure_parent(&child).expect("could not ensure parent directory");
            assert!(parent.exists());

            // remove_dir
            assert!(parent.exists());
            remove_dir(&parent).unwrap();
            assert!(!parent.exists());
            assert!(remove_dir(&parent).is_err());
            let (file, _) = dir.safe_add_last("file.txt");
            std::fs::File::create(file.to_path_buf()).unwrap();
            assert!(remove_dir(&file).is_err());

            // remove_dir_all
            assert!(remove_dir_all(&file).is_err());
            assert!(dir.0.exists());
            remove_dir_all(&dir.0).unwrap();
            assert!(remove_dir_all(&dir.0).is_err());
            assert!(!parent.exists());
            assert!(remove_dir(&parent).is_err());
        });

        cleanup_sandbox(sandbox);

        assert!(result.is_ok())
    }
}
