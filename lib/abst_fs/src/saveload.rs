use super::{ensure_parent, error_context, generr, inerr, unkext, AbstPath, Error, ObjectType};

use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug, PartialEq)]
enum Ext {
    Bin,
    Toml,
}
fn get_ext(path: &AbstPath) -> Option<Ext> {
    let ext = path.extension()?;
    match ext.to_ascii_lowercase().as_str() {
        "bin" => Some(Ext::Bin),
        "toml" => Some(Ext::Toml),
        _ => None,
    }
}

/// Load the data from a file, interpreting the content of the file based on the
/// extension (see [`Ext`] for the possible extensions) of the file and deserializing
/// the content to the generic type T
pub fn load<T: DeserializeOwned>(path: &AbstPath) -> Result<T, Error> {
    let errctx = error_context(format!("could not load file at path {}", path));
    if !path.exists() {
        return Err(generr(errctx("open file"), "file doesn't exist"));
    }
    if path.object_type() != Some(ObjectType::File) {
        return Err(generr(errctx("open file"), "object at path is not a file"));
    }

    match get_ext(path) {
        Some(Ext::Toml) => {
            let serialized = std::fs::read_to_string(path.to_path_buf())
                .map_err(inerr(errctx("read content to string")))?;
            toml::from_str(&serialized).map_err(inerr(errctx("deserialize content from toml")))
        }
        Some(Ext::Bin) => {
            let file =
                std::fs::File::open(path.to_path_buf()).map_err(inerr(errctx("open file")))?;
            bincode::deserialize_from(file)
                .map_err(inerr(errctx("deserialize content from binary")))
        }
        None => Err(unkext(path)),
    }
}
/// Save a serializable data structure of generic type T to a file, encoding the
/// serialized data based on the extension of the file (see [`Ext`] for the possible
/// extensions)
pub fn save<T: Serialize>(path: &AbstPath, content: &T) -> Result<(), Error> {
    let errctx = error_context(format!("could not save file at path {}", path));
    match get_ext(path) {
        Some(Ext::Toml) => {
            let serialized =
                toml::to_string(content).map_err(inerr(errctx("serialize content to toml")))?;
            ensure_parent(path)?;
            std::fs::write(path.to_path_buf(), serialized)
                .map_err(inerr(errctx("write content to file")))
        }
        Some(Ext::Bin) => {
            let serialized = bincode::serialize(content)
                .map_err(inerr(errctx("serialize content to binary")))?;
            ensure_parent(path)?;
            std::fs::write(path.to_path_buf(), serialized)
                .map_err(inerr(errctx("write content to file")))
        }
        None => Err(unkext(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::{get_ext, load, save, AbstPath, Ext};
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Debug, PartialEq, Deserialize, Serialize)]
    struct TestStruct {
        byte: u8,
        int: i64,
        string: String,
        vec: Vec<TestStruct>,
    }
    impl Default for TestStruct {
        fn default() -> Self {
            let depth2 = TestStruct {
                byte: 255,
                int: 3141592653589793238,
                string: String::from("depth2struct"),
                vec: Vec::new(),
            };
            let depth1_a = TestStruct {
                byte: 127,
                int: 2718281828459045235,
                string: String::from("depth1_astruct"),
                vec: vec![depth2],
            };
            let depth1_b = TestStruct {
                byte: 57,
                int: 1618033988749894848,
                string: String::from("depth1_bstruct"),
                vec: Vec::new(),
            };
            TestStruct {
                byte: 0,
                int: 1414213562373095048,
                string: String::from("depth0struct"),
                vec: vec![depth1_a, depth1_b],
            }
        }
    }

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
        let path_bf = PathBuf::from("/tmp/bbup-test-abst_fs-saveload");
        let path = (AbstPath::from(&path_bf), path_bf);
        //	make sure the path means actually what I think it mean
        assert_eq!(path.0.to_path_buf(), path.1);

        if path.1.exists() {
            panic!(
                "path [{:?}] should not exist in order to run this test, but it does exist!",
                path.1
            );
        }

        let result = std::panic::catch_unwind(|| {
            std::fs::create_dir(&path.1).unwrap();

            let (file_bin, _) = path.safe_add_last("file.bin");
            assert_eq!(get_ext(&file_bin), Some(Ext::Bin));
            assert!(load::<TestStruct>(&file_bin).is_err());
            save(&file_bin, &TestStruct::default()).unwrap();
            assert_eq!(
                load::<TestStruct>(&file_bin).unwrap(),
                TestStruct::default()
            );

            let (file_toml, _) = path.safe_add_last("file.toml");
            assert_eq!(get_ext(&file_toml), Some(Ext::Toml));
            assert!(load::<TestStruct>(&file_toml).is_err());
            save(&file_toml, &TestStruct::default()).unwrap();
            assert_eq!(
                load::<TestStruct>(&file_toml).unwrap(),
                TestStruct::default()
            );

            let (file_txt, file_txt_pb) = path.safe_add_last("file.txt");
            assert_eq!(get_ext(&file_txt), None);
            assert!(load::<TestStruct>(&file_txt).is_err());
            assert!(save(&file_txt, &TestStruct::default()).is_err());
            std::fs::write(
                file_txt_pb,
                bincode::serialize(&TestStruct::default()).unwrap(),
            )
            .unwrap();
            assert!(load::<TestStruct>(&file_txt).is_err());

            let (symlink, symlink_pb) = path.safe_add_last("symlink");
            assert_eq!(get_ext(&symlink), None);
            assert!(load::<TestStruct>(&symlink).is_err());
            assert!(save(&symlink, &TestStruct::default()).is_err());
            std::os::unix::fs::symlink("some/path/to/somewhere", symlink_pb).unwrap();
            assert!(load::<TestStruct>(&symlink).is_err());

            let (extensionless_file, _) = path.safe_add_last("extensionless_file");
            assert_eq!(get_ext(&extensionless_file), None);
            assert!(load::<TestStruct>(&extensionless_file).is_err());
            assert!(save(&extensionless_file, &TestStruct::default()).is_err());
            std::fs::write(
                extensionless_file.to_path_buf(),
                bincode::serialize(&TestStruct::default()).unwrap(),
            )
            .unwrap();
            assert!(load::<TestStruct>(&extensionless_file).is_err());

            let (non_existing_file, _) = path.safe_add_last("non_existing_file.bin");
            assert_eq!(get_ext(&non_existing_file), Some(Ext::Bin));
            assert!(load::<TestStruct>(&non_existing_file).is_err());
        });

        if path.1.exists() {
            std::fs::remove_dir_all(&path.1).unwrap();
        }

        assert!(result.is_ok())
    }
}
