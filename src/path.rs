use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(
        "Trying to create an AbstractPath from an absolute path buf, which is not allowed\npath buf: {path:?}"
    )]
    FromAbsolutePath { path: std::path::PathBuf },

    #[error(
        "Trying to remove a prefix ({prefix:?}) that doesn't match on abstract path ({abst_path:?})"
    )]
    PrefixMismatch {
        abst_path: Vec<String>,
        prefix: Vec<String>,
    },
}

#[derive(Clone, PartialEq)]
pub enum EntryType {
    Dir,
    FileType(FileType),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum FileType {
    File,
    SymLink,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AbstractPath {
    components: VecDeque<String>,
}

impl AbstractPath {
    pub fn empty() -> AbstractPath {
        AbstractPath {
            components: VecDeque::new(),
        }
    }
    pub fn from(path: std::path::PathBuf) -> Result<AbstractPath, Error> {
        if path.is_absolute() {
            return Err(Error::FromAbsolutePath { path });
            // return Err(std::io::Error::new(
            //     std::io::ErrorKind::Other,
            //     "cannot create abstract path from absolute path",
            // ));
        }

        let mut components: VecDeque<String> = VecDeque::new();

        for component in path.components() {
            components.push_back(component.as_os_str().force_to_string());
        }

        Ok(AbstractPath { components })
    }
    pub fn to_path_buf(&self) -> std::path::PathBuf {
        if self.components.len() == 0 {
            return std::path::PathBuf::from("");
        }

        let mut output = std::path::PathBuf::from(&self.components[0]);

        for i in 1..self.components.len() {
            output.push(&self.components[i]);
        }

        output
    }
    pub fn pop_front(&mut self) -> Option<String> {
        self.components.pop_front()
    }
    pub fn push_front(&mut self, arg: String) {
        self.components.push_front(arg);
    }
    pub fn push_back(&mut self, arg: String) {
        self.components.push_back(arg);
    }
    pub fn pop_back(&mut self) -> Option<String> {
        self.components.pop_back()
    }

    pub fn size(&self) -> usize {
        self.components.len()
    }

    pub fn join(&self, path: &AbstractPath) -> AbstractPath {
        let mut new_components = self.components.clone();
        new_components.append(&mut VecDeque::<String>::from(path.to_vec()));
        AbstractPath {
            components: new_components,
        }
    }

    pub fn starts_with(&self, prefix: &AbstractPath) -> bool {
        for (i, component) in prefix.to_vec().into_iter().enumerate() {
            if !self.components[i].eq(&component) {
                return false;
            }
        }

        true
    }
    pub fn strip_prefix(&self, prefix: &AbstractPath) -> Result<AbstractPath, Error> {
        let mut new_components = self.components.clone();
        for component in &prefix.to_vec() {
            let tail = new_components.pop_back();
            match tail {
                Some(val) if val.eq(component) => {}
                _ => {
                    return Err(Error::PrefixMismatch {
                        abst_path: Vec::<String>::from(self.components.clone()),
                        prefix: prefix.to_vec(),
                    })
                }
            }
        }
        Ok(AbstractPath {
            components: new_components,
        })
    }
    pub fn to_vec(&self) -> Vec<String> {
        Vec::<String>::from(self.components.clone())
    }
}

impl ToString for AbstractPath {
    fn to_string(&self) -> String {
        if self.components.len() == 0 {
            return String::from("");
        }

        let mut output = self.components[0].clone();
        for i in 1..self.components.len() {
            output.push(std::path::MAIN_SEPARATOR);
            output += self.components[i].as_str();
        }

        output
    }
}

pub trait ForceToString {
    fn force_to_string(&self) -> String;
}

impl ForceToString for std::ffi::OsStr {
    fn force_to_string(&self) -> String {
        self.to_str()
            .expect(
                format!(
					"Broken OsString: could not convert from os string to valid utf8\nos string: {:?}",
					self
				)
                .as_str(),
            )
            .to_string()
    }
}

pub trait ForceFilename {
    fn force_file_name(&self) -> String;
}
impl ForceFilename for std::path::PathBuf {
    fn force_file_name(&self) -> String {
        self.file_name()
            .expect(
                format!(
                    "Broken path buf: unable to get file name (might end with ..)\npath buf: {:?}",
                    self
                )
                .as_str(),
            )
            .force_to_string()
    }
}

pub trait PathType {
    fn get_type(&self) -> EntryType;
}

impl PathType for std::path::PathBuf {
    fn get_type(&self) -> EntryType {
        if self.is_dir() {
            return EntryType::Dir;
        } else if self.is_symlink() {
            return EntryType::FileType(FileType::SymLink);
        } else if self.is_file() {
            return EntryType::FileType(FileType::File);
        } else {
            panic!("Unrecognised filesystem entry: not a dir, not a symlink and not a file\npath: {:?}", self)
        }
    }
}
