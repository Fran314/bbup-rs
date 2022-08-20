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

    #[test]
    fn test() {
        let path_bf = PathBuf::from("/tmp/bbup-test-abst_fs-directory");
        let path = AbstPath::from(&path_bf);
        //	make sure paths mean actually what I think they mean
        assert_eq!(path.to_path_buf(), path_bf);

        if path_bf.exists() {
            panic!(
                "path [{path_bf:?}] should not exist in order to run this test, but it does exist!"
            );
        }

        let result = std::panic::catch_unwind(|| {
            // create_dir
            assert!(!path.exists());
            create_dir(&path).expect("could not create dir");
            assert!(path.exists());
            assert_eq!(path.object_type(), Some(ObjectType::Dir));

            // list_dir_content
            assert_eq!(list_dir_content(&path).unwrap(), vec![]);

            let file1 = path.add_last("file1.txt");
            assert_eq!(file1.to_path_buf(), path_bf.join("file1.txt"));
            let file2 = path.add_last("file2.png");
            assert_eq!(file2.to_path_buf(), path_bf.join("file2.png"));
            let symlink1 = path.add_last("symlink1.ln");
            assert_eq!(symlink1.to_path_buf(), path_bf.join("symlink1.ln"));
            let symlink2 = path.add_last("symlink2");
            assert_eq!(symlink2.to_path_buf(), path_bf.join("symlink2"));
            let dir1 = path.add_last("dir1");
            assert_eq!(dir1.to_path_buf(), path_bf.join("dir1"));
            let dir2 = path.add_last("dir2");
            assert_eq!(dir2.to_path_buf(), path_bf.join("dir2"));
            std::fs::File::create(file1.to_path_buf()).unwrap();
            std::fs::File::create(file2.to_path_buf()).unwrap();
            std::os::unix::fs::symlink(".", symlink1.to_path_buf()).unwrap();
            std::os::unix::fs::symlink(".", symlink2.to_path_buf()).unwrap();
            create_dir(&dir1).unwrap();
            create_dir(&dir2).unwrap();

            let mut dir_list = list_dir_content(&path)
                .unwrap()
                .into_iter()
                .map(|path| path.to_string())
                .collect::<Vec<String>>();
            dir_list.sort();

            let mut artificialdir_list = vec![file1, file2, symlink1, symlink2, dir1, dir2]
                .into_iter()
                .map(|path| path.to_string())
                .collect::<Vec<String>>();
            artificialdir_list.sort();

            assert_eq!(dir_list, artificialdir_list);

            // make_clean_dir
            assert!(path.exists());
            assert_ne!(list_dir_content(&path).unwrap(), vec![]);

            make_clean_dir(&path).unwrap();
            assert!(path.exists());
            assert_eq!(list_dir_content(&path).unwrap(), vec![]);

            make_clean_dir(&path).unwrap();
            assert!(path.exists());
            assert_eq!(list_dir_content(&path).unwrap(), vec![]);

            remove_dir(&path).unwrap();
            assert!(!path.exists());

            make_clean_dir(&path).unwrap();
            assert!(path.exists());
            assert_eq!(list_dir_content(&path).unwrap(), vec![]);

            // ensure_parent
            let parent = path.add_last("test").add_last("something");
            let child = parent.add_last("file.txt");
            //	make sure paths mean actually what I think they mean
            assert_eq!(parent.to_path_buf(), path_bf.join("test").join("something"));
            assert_eq!(
                child.to_path_buf(),
                path_bf.join("test").join("something").join("file.txt")
            );
            assert!(!parent.exists());
            ensure_parent(&child).expect("could not ensure parent directory");
            assert!(parent.exists());

            // remove_dir
            assert!(parent.exists());
            remove_dir(&parent).unwrap();
            assert!(!parent.exists());
            assert!(remove_dir(&parent).is_err());
            let file = path.add_last("file.txt");
            assert_eq!(file.to_path_buf(), path_bf.join("file.txt"));
            std::fs::File::create(file.to_path_buf()).unwrap();
            assert!(remove_dir(&file).is_err());

            // remove_dir_all
            assert!(remove_dir_all(&file).is_err());
            assert!(path.exists());
            remove_dir_all(&path).unwrap();
            assert!(!parent.exists());
            assert!(remove_dir(&parent).is_err());
        });

        if path_bf.exists() {
            std::fs::remove_dir_all(path_bf).unwrap();
        }

        assert!(result.is_ok())
    }
}
