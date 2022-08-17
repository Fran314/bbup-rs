use std::{
    collections::VecDeque,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::ForceToString;

pub const ABST_OBJ_HEADER: &str = "[[bbup abstract symlink object]]";

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

    pub fn append(&self, AbstPath(appendix): &AbstPath) -> AbstPath {
        let AbstPath(mut path) = self.clone();
        path.append(&mut appendix.clone());
        AbstPath(path)
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

    pub fn exists(&self) -> bool {
        self.to_path_buf().exists()
    }
    pub fn object_type(&self) -> Option<ObjectType> {
        let path = self.to_path_buf();

        if !path.exists() {
            None
        } else if path.is_symlink() {
            Some(ObjectType::SymLink)
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
    pub fn parent(&self) -> Option<AbstPath> {
        // NOTE: while this could technically be easily done by just popping the
        //	last component of the vec in most cases, it might not be this easy
        //	in other cases, eg: if a path is ["c:", "/"], it is not true that
        //	the parent is ["c:"], so it's better to rely on the built in parent
        //	function to work around os specific cases
        Some(AbstPath::from(self.to_path_buf().parent()?))
    }
    pub fn file_name(&self) -> Option<String> {
        // NOTE: same as `fn parent(&self)`
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
