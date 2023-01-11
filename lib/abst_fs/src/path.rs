use super::ABST_OBJ_HEADER;

use std::{
    collections::VecDeque,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

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

#[derive(PartialEq, Debug)]
pub enum ObjectType {
    File,
    SymLink,
    Dir,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbstPath(VecDeque<String>);
impl AbstPath {
    pub fn empty() -> AbstPath {
        AbstPath(VecDeque::new())
    }
    pub fn single<S: std::string::ToString>(path: S) -> AbstPath {
        AbstPath(VecDeque::from([path.to_string()]))
    }
    pub fn from<T: AsRef<Path>>(path: T) -> AbstPath {
        let components: Vec<String> = path
            .as_ref()
            .components()
            .map(|comp| comp.as_os_str().force_to_string())
            .collect();

        AbstPath(components.into())
    }
    pub fn to_path_buf(&self) -> PathBuf {
        let AbstPath(abst_path) = self;
        PathBuf::from_iter(abst_path)
    }

    pub fn len(&self) -> usize {
        let AbstPath(path) = self;
        path.len()
    }
    pub fn is_empty(&self) -> bool {
        let AbstPath(path) = self;
        path.is_empty()
    }
    pub fn get(&self, pos: usize) -> Option<&String> {
        let AbstPath(path) = self;
        path.get(pos)
    }

    pub fn add_first<S: std::string::ToString>(&self, prefix: S) -> AbstPath {
        let AbstPath(mut path) = self.clone();
        path.push_front(prefix.to_string());
        AbstPath(path)
    }
    pub fn add_last<S: std::string::ToString>(&self, suffix: S) -> AbstPath {
        let AbstPath(mut path) = self.clone();
        path.push_back(suffix.to_string());
        AbstPath(path)
    }
    pub fn strip_first(&self) -> AbstPath {
        let AbstPath(mut path) = self.clone();
        path.pop_front();
        AbstPath(path)
    }
    pub fn strip_last(&self) -> AbstPath {
        let AbstPath(mut path) = self.clone();
        path.pop_back();
        AbstPath(path)
    }
    pub fn append(&self, AbstPath(appendix): &AbstPath) -> AbstPath {
        let AbstPath(mut path) = self.clone();
        path.append(&mut appendix.clone());
        AbstPath(path)
    }

    pub fn parent(&self) -> Option<AbstPath> {
        // NOTE: while this could technically be easily done by just popping the
        //	last component of the vec in most cases, it might not be this easy
        //	in other cases, eg: if a path is ["c:", "/"], it is not true that
        //	the parent is ["c:"], so it's better to rely on the built in parent
        //	function to work around os specific cases
        Some(AbstPath::from(self.to_path_buf().parent()?))
    }
    pub fn file_name(&self) -> Option<String> {
        // NOTE: same reasoning as the comment for `fn parent(&self)`
        Some(self.to_path_buf().file_name()?.force_to_string())
    }
    pub fn extension(&self) -> Option<&str> {
        let AbstPath(path) = self;
        let last = path.get(path.len() - 1)?;
        let last_dot_occurrence = last.rfind('.')?;
        let ext = &last[last_dot_occurrence + 1..];
        match ext.is_empty() {
            true => None,
            false => Some(ext),
        }
    }

    pub fn exists(&self) -> bool {
        let path = self.to_path_buf();
        // The reason why we have to do this `path.is_symlink()` thing is because
        //	by default PathBuf::exists() follows symlink so if you're checking a
        //	path pointing to a symlink and the symlink's endpoint doesn't exist,
        //	PathBuf::exists() will tell you that the path doesn't exists.
        // Furthermore, PathBuf::exists() doesn't provide a function that does
        //	what exists does without following symlinks
        path.is_symlink() || path.exists()
    }
    pub fn object_type(&self) -> Option<ObjectType> {
        let path = self.to_path_buf();

        // The reason behind why we check first if the path is a symlink and
        //	then if it exists is the same reason for the weird implementation of
        //	AbstPath::exists() so go see that
        if path.is_symlink() {
            Some(ObjectType::SymLink)
        } else if !path.exists() {
            None
        } else if path.is_dir() {
            Some(ObjectType::Dir)
        } else if path.is_file() {
            let mut file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(_) => return Some(ObjectType::File),
            };
            let mut header = vec![0u8; ABST_OBJ_HEADER.len()];
            match file.read_exact(&mut header) {
                Ok(_) => {}
                Err(_) => return Some(ObjectType::File),
            }
            if header.eq(ABST_OBJ_HEADER.as_bytes()) {
                Some(ObjectType::SymLink)
            } else {
                Some(ObjectType::File)
            }
        } else {
            panic!(
                "Foreign file system object. Not a directory, a file nor a symlink, at path: {}",
                self
            )
        }
    }
}
impl IntoIterator for AbstPath {
    type Item = String;
    type IntoIter = std::collections::vec_deque::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        let AbstPath(path) = self;
        path.into_iter()
    }
}
impl<'a> IntoIterator for &'a AbstPath {
    type Item = &'a String;
    type IntoIter = std::collections::vec_deque::Iter<'a, String>;

    fn into_iter(self) -> Self::IntoIter {
        let AbstPath(path) = self;
        path.iter()
    }
}
impl std::fmt::Display for AbstPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let AbstPath(abst_path) = self;
        #[cfg(unix)]
        let string = {
            PathBuf::from_iter(abst_path.iter().map(|comp| comp.replace('\\', "/")))
                .force_to_string()
        };
        #[cfg(windows)]
        let string = {
            PathBuf::from_iter(abst_path)
                .force_to_string()
                .replace('\\', "/")
        };
        write!(f, "{}", string)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum Endpoint {
    Unix(String),
    Windows(bool, String),
}
impl Endpoint {
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        match self {
            Endpoint::Unix(endpath) => {
                // OS byte
                bytes.push(0);

                // Endpath bytes
                bytes.append(&mut endpath.as_bytes().to_vec());
            }
            Endpoint::Windows(is_dir, endpath) => {
                // OS byte
                bytes.push(1);

                // Is_dir byte
                match is_dir {
                    true => bytes.push(0),
                    false => bytes.push(1),
                }

                // Endpath bytes
                bytes.append(&mut endpath.as_bytes().to_vec());
            }
        }

        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::{AbstPath, Endpoint};
    use std::collections::VecDeque;

    #[test]
    fn force_to_string() {
        use super::ForceToString;
        use std::ffi::OsStr;
        use std::path::{Path, PathBuf};

        let path = "some/path_/with!*àèéìòùç/weird€$£/charset([{}])==";
        assert_eq!(Path::new(path).force_to_string().as_str(), path);
        assert_eq!(PathBuf::from(path).force_to_string().as_str(), path);
        assert_eq!(OsStr::new(path).force_to_string().as_str(), path);
    }

    #[test]
    fn empty() {
        assert_eq!(AbstPath(VecDeque::from([])), AbstPath::empty());
    }

    #[test]
    fn single() {
        assert_eq!(
            AbstPath(VecDeque::from([String::from("test")])),
            AbstPath::single("test")
        );
        // single vs empty
        assert_ne!(AbstPath::empty(), AbstPath::single(""));
        assert_eq!(
            AbstPath(VecDeque::from([String::from("")])),
            AbstPath::single("")
        );
    }

    #[test]
    fn from() {
        assert_eq!(
            AbstPath(VecDeque::from([
                String::from("path"),
                String::from("to"),
                String::from("somewhere")
            ])),
            AbstPath::from("path/to/somewhere")
        );

        assert_eq!(
            AbstPath::from("path/to/directory"),
            AbstPath::from("path/to/directory/")
        );
        assert_ne!(
            AbstPath::from("path/to/somewhere"),
            AbstPath::from("/path/to/somewhere")
        );

        // from vs single
        assert_eq!(AbstPath::single("test"), AbstPath::from("test"));
        assert_ne!(AbstPath::single(""), AbstPath::from(""));
        assert_ne!(
            AbstPath(VecDeque::from([String::from(""),])),
            AbstPath::from("")
        );
        assert_eq!(AbstPath(VecDeque::from([])), AbstPath::from(""));
        assert_ne!(AbstPath::single("test/path"), AbstPath::from("test/path"));
    }

    #[test]
    fn to_path_buf() {
        use std::path::PathBuf;
        let path = "/home/user/Desktop/something";
        assert_eq!(AbstPath::from(path).to_path_buf(), PathBuf::from(path));

        let path = "test/on/non/absolute/path";
        assert_eq!(AbstPath::from(path).to_path_buf(), PathBuf::from(path));

        let path = "./test/on/relative/path";
        assert_eq!(AbstPath::from(path).to_path_buf(), PathBuf::from(path));

        let path = "./test/with/weird/charset/-.--+*+/àèéìòùç/$%&£()[]{}";
        assert_eq!(AbstPath::from(path).to_path_buf(), PathBuf::from(path));

        let path = "";
        assert_eq!(AbstPath::from(path).to_path_buf(), PathBuf::from(path));
    }

    #[test]
    fn len() {
        assert_eq!(AbstPath::empty().len(), 0);
        assert_eq!(AbstPath::single("a/b/c/d/e/f").len(), 1);
        assert_eq!(AbstPath::from("a/b/c/d/e/f").len(), 6);

        let vec = VecDeque::from([
            String::from("a"),
            String::from("b"),
            String::from("c"),
            String::from("d"),
            String::from("e"),
            String::from("f"),
            String::from("g"),
            String::from("h"),
        ]);
        assert_eq!(vec.len(), AbstPath(vec).len())
    }

    #[test]
    fn is_empty() {
        assert!(AbstPath::empty().is_empty());
        assert!(AbstPath::from("").is_empty());

        assert!(!AbstPath::single("test").is_empty());
        assert!(!AbstPath::from("test").is_empty());
    }

    #[test]
    fn get() {
        let first = String::from("first");
        let second = String::from("second");
        let third = String::from("third");
        let fourth = String::from("fourth");
        let last = String::from("last");

        let path = AbstPath(VecDeque::from([
            first.clone(),
            second.clone(),
            third.clone(),
            fourth.clone(),
            last.clone(),
        ]));

        assert_eq!(path.get(0).unwrap(), &first);
        assert_eq!(path.get(1).unwrap(), &second);
        assert_eq!(path.get(2).unwrap(), &third);
        assert_eq!(path.get(3).unwrap(), &fourth);
        assert_eq!(path.get(path.len() - 1).unwrap(), &last);

        assert_eq!(path.get(path.len()), None);
    }

    #[test]
    fn add_first() {
        use std::path::PathBuf;

        // add first on middle path
        let path = "some/path/to/somewhere";
        let parent1 = "parent1";
        let parent2 = "parent2";
        assert_eq!(
            AbstPath::from(path).add_first(parent1).to_path_buf(),
            PathBuf::from(parent1).join(path)
        );
        assert_eq!(
            AbstPath::from(path)
                .add_first(parent1)
                .add_first(parent2)
                .to_path_buf(),
            PathBuf::from(parent2).join(parent1).join(path)
        );

        // add first on absolute path (weird quirk from std::path)
        let absolute_path = "/some/absolute/path";
        let parent = "parent";
        assert_eq!(
            AbstPath::from(absolute_path)
                .add_first(parent)
                .to_string()
                .as_str(),
            absolute_path
        );
        assert_eq!(
            AbstPath::from(absolute_path)
                .add_first(parent)
                .to_path_buf(),
            PathBuf::from(absolute_path)
        );
        assert_eq!(
            AbstPath::from(absolute_path)
                .add_first(parent)
                .to_path_buf(),
            PathBuf::from(parent).join(absolute_path)
        );
    }

    #[test]
    fn add_last() {
        use std::path::PathBuf;

        let path = "some/path/to/somewhere";
        let child1 = "child1";
        let child2 = "child2";
        assert_eq!(
            AbstPath::from(path).add_last(child1).to_path_buf(),
            PathBuf::from(path).join(child1)
        );
        assert_eq!(
            AbstPath::from(path)
                .add_last(child1)
                .add_last(child2)
                .to_path_buf(),
            PathBuf::from(path).join(child1).join(child2)
        );
    }

    #[test]
    fn strip_first() {
        use std::path::PathBuf;

        let path = "some/path/to/somewhere";
        let parent = "parent";
        let full_path = format!("{}/{}", parent, path);
        assert_eq!(
            AbstPath::from(full_path).strip_first().to_path_buf(),
            PathBuf::from(path)
        );

        let path = "some/path/to/somewhere";
        let parent1 = "parent1";
        let parent2 = "parent2";
        let full_path = format!("{}/{}/{}", parent2, parent1, path);
        assert_eq!(
            AbstPath::from(full_path)
                .strip_first()
                .strip_first()
                .to_path_buf(),
            PathBuf::from(path)
        );
    }

    #[test]
    fn strip_last() {
        use std::path::PathBuf;

        let path = "some/path/to/somewhere";
        let child = "child";
        let full_path = format!("{}/{}", path, child);
        assert_eq!(
            AbstPath::from(full_path).strip_last().to_path_buf(),
            PathBuf::from(path)
        );

        let path = "some/path/to/somewhere";
        let child1 = "child1";
        let child2 = "child2";
        let full_path = format!("{}/{}/{}", path, child1, child2);
        assert_eq!(
            AbstPath::from(full_path)
                .strip_last()
                .strip_last()
                .to_path_buf(),
            PathBuf::from(path)
        );
    }

    #[test]
    fn append() {
        use std::path::PathBuf;

        let parent = "some/path/to/somewhere";
        let child = "here/is/a/subpath";
        assert_eq!(
            AbstPath::from(parent)
                .append(&AbstPath::from(child))
                .to_path_buf(),
            PathBuf::from(parent).join(child)
        );

        // weird quirk from std::path
        let parent = "some/path/to/somewhere";
        let child = "/some/absolute/path";
        assert_eq!(
            AbstPath::from(parent)
                .append(&AbstPath::from(child))
                .to_path_buf(),
            PathBuf::from(parent).join(child)
        );
        assert_eq!(
            AbstPath::from(parent)
                .append(&AbstPath::from(child))
                .to_string()
                .as_str(),
            child
        );
    }

    #[test]
    fn parent() {
        let path = "path/to/somewhere";
        let child = "child";
        assert_eq!(
            AbstPath::from(path).add_last(child).parent().unwrap(),
            AbstPath::from(path)
        )
    }

    #[test]
    fn file_name() {
        let file = "supersecretpassword.txt";
        let path = format!("path/to/somewhere/{file}");
        assert_eq!(AbstPath::from(path).file_name().unwrap().as_str(), file);
    }

    #[test]
    fn extension() {
        let path = "path/to/some/file.txt";
        assert_eq!(AbstPath::from(path).extension(), Some("txt"));

        let path = "path/to/some/file.tar.gz";
        assert_eq!(AbstPath::from(path).extension(), Some("gz"));

        let path = "path/to/some/file";
        assert_eq!(AbstPath::from(path).extension(), None);

        let path = "path/to/some/file.";
        assert_eq!(AbstPath::from(path).extension(), None);
    }

    #[test]
    fn into_iter() {
        let vec = VecDeque::from([
            String::from("path"),
            String::from("to"),
            String::from("some"),
            String::from("test"),
            String::from("directory"),
            String::from("very"),
            String::from("extremely"),
            String::from("long"),
            String::from("path"),
        ]);
        let path = AbstPath(vec.clone());
        let mut vec_iter = vec.into_iter();

        for comp in path {
            assert_eq!(comp, vec_iter.next().unwrap())
        }
        assert_eq!(vec_iter.next(), None);
    }

    #[test]
    fn into_iter_ref() {
        let vec = VecDeque::from([
            String::from("path"),
            String::from("to"),
            String::from("some"),
            String::from("test"),
            String::from("directory"),
            String::from("very"),
            String::from("extremely"),
            String::from("long"),
            String::from("path"),
        ]);
        let path = &AbstPath(vec.clone());
        let mut vec_iter = vec.iter();

        for comp in path {
            assert_eq!(comp, vec_iter.next().unwrap())
        }
        assert_eq!(vec_iter.next(), None);
    }

    #[test]
    fn to_string() {
        let path = "/home/user/Desktop/something";
        assert_eq!(AbstPath::from(path).to_string().as_str(), path);

        let path = "test/on/non/absolute/path";
        assert_eq!(AbstPath::from(path).to_string().as_str(), path);

        let path = "./test/on/relative/path";
        assert_eq!(AbstPath::from(path).to_string().as_str(), path);

        let path = "./test/with/weird/charset/-.--+*+/àèéìòùç/$%&£()[]{}";
        assert_eq!(AbstPath::from(path).to_string().as_str(), path);

        assert_eq!(AbstPath::empty().to_string().as_str(), "");
        assert_eq!(AbstPath::single("").to_string().as_str(), "");
        assert_eq!(AbstPath::from("").to_string().as_str(), "");
    }

    #[test]
    fn exists_and_type() {
        use super::{ObjectType, ABST_OBJ_HEADER};
        use std::path::PathBuf;
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

        let path_bf = PathBuf::from("/tmp/bbup-test-abst_fs-path-exists");
        let path = (AbstPath::from(&path_bf), path_bf);
        //	make sure the path means actually what I think it mean
        assert_eq!(path.0.to_path_buf(), path.1);
        assert!(!path.1.exists());
        std::fs::create_dir(&path.1).unwrap();

        let result = std::panic::catch_unwind(|| {
            let file = path.safe_add_last("file.txt");
            assert!(!file.0.exists());
            assert_eq!(file.0.object_type(), None);
            std::fs::File::create(&file.1).unwrap();
            assert!(file.0.exists());
            assert_eq!(file.0.object_type(), Some(ObjectType::File));

            let dir = path.safe_add_last("dir");
            assert!(!dir.0.exists());
            assert_eq!(dir.0.object_type(), None);
            std::fs::create_dir(&dir.1).unwrap();
            assert!(dir.0.exists());
            assert_eq!(dir.0.object_type(), Some(ObjectType::Dir));

            let symlink = path.safe_add_last("symlink.ln");
            assert!(!symlink.0.exists());
            assert_eq!(symlink.0.object_type(), None);
            std::os::unix::fs::symlink("some/path/to/somwhere", &symlink.1).unwrap();
            assert!(symlink.0.exists());
            assert_eq!(symlink.0.object_type(), Some(ObjectType::SymLink));

            let abst_symlink = path.safe_add_last("abst_symlink.ln");
            assert!(!abst_symlink.0.exists());
            assert_eq!(abst_symlink.0.object_type(), None);
            std::fs::write(
                abst_symlink.1,
                format!("{ABST_OBJ_HEADER}\nunix\nsome/path/to/somewhere").as_bytes(),
            )
            .unwrap();
            assert!(abst_symlink.0.exists());
            assert_eq!(abst_symlink.0.object_type(), Some(ObjectType::SymLink));
        });

        std::fs::remove_dir_all(&path.1).unwrap();

        assert!(result.is_ok())
    }

    #[test]
    fn as_bytes() {
        assert_ne!(
            Endpoint::Unix(String::from("some/path/to/somewhere")).as_bytes(),
            Endpoint::Unix(String::from("path/to/somewhere/else")).as_bytes()
        );

        assert_ne!(
            Endpoint::Windows(true, String::from("some/path/to/somewhere")).as_bytes(),
            Endpoint::Windows(true, String::from("path/to/somewhere/else")).as_bytes()
        );
        assert_ne!(
            Endpoint::Windows(false, String::from("some/path/to/somewhere")).as_bytes(),
            Endpoint::Windows(false, String::from("path/to/somewhere/else")).as_bytes()
        );
        assert_ne!(
            Endpoint::Windows(true, String::from("some/path/to/somewhere")).as_bytes(),
            Endpoint::Windows(false, String::from("some/path/to/somewhere")).as_bytes()
        );

        assert_ne!(
            Endpoint::Unix(String::from("some/path/to/somewhere")).as_bytes(),
            Endpoint::Windows(true, String::from("some/path/to/somewhere")).as_bytes()
        );
        assert_ne!(
            Endpoint::Unix(String::from("some/path/to/somewhere")).as_bytes(),
            Endpoint::Windows(false, String::from("some/path/to/somewhere")).as_bytes()
        );
    }
}
