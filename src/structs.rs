use serde::{Deserialize, Serialize};

use crate::path::{AbstractPath, FileType};

//--- STUFF TO SORT ---//
#[derive(Serialize, Deserialize, Debug, Clone)]
/// Enumerate the types of addition that can be done
pub enum Adding {
    Dir,

    /// `FileType(FileType::File | FileType::SymLink, hash)` where `hash` is the hash of the content of the file added
    FileType(FileType, String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Enumerate the types of edit that can be done
pub enum Editing {
    /// `FileType(FileType::File | FileType::SymLink, hash)` where `hash` is the hash of the content of the file added
    FileType(FileType, String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Enumerate the types of removal that can be done
pub enum Removing {
    Dir,
    FileType(FileType),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Wrapper containint the type of the change done
pub enum ChangeType {
    Added(Adding),
    Edited(Editing),
    Removed(Removing),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Struct containing all the necessary information
/// on a change that occurred between hashtrees
pub struct Change {
    /// Path where the change occurred
    pub path: AbstractPath,

    /// Type of change that occurred
    pub change_type: ChangeType,
}
//--- ---//

//--- COMMON STUFF ---//
pub type Delta = Vec<Change>;
pub trait PrettyPrint {
    fn pretty_print(&self, indent: u8) -> String;
}
impl PrettyPrint for Delta {
    fn pretty_print(&self, indent: u8) -> String {
        let ind = String::from("\t".repeat(indent as usize));
        let mut output = String::new();
        for i in 0..self.len() {
            output += ind.as_str();

            output += match self[i].change_type {
                ChangeType::Added(Adding::Dir) => "+++  dir   ",
                ChangeType::Added(Adding::FileType(FileType::File, _)) => "+++  file  ",
                ChangeType::Added(Adding::FileType(FileType::SymLink, _)) => "+++  sylk  ",
                ChangeType::Edited(Editing::FileType(FileType::File, _)) => "~~~  file  ",
                ChangeType::Edited(Editing::FileType(FileType::SymLink, _)) => "~~~  sylk  ",
                ChangeType::Removed(Removing::Dir) => "---  dir   ",
                ChangeType::Removed(Removing::FileType(FileType::File)) => "---  file  ",
                ChangeType::Removed(Removing::FileType(FileType::SymLink)) => "---  sylk  ",
            };
            output += self[i].path.to_string().as_str();
            if i != self.len() - 1 {
                output += "\n";
            }
        }
        output
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Commit {
    pub commit_id: String,
    pub delta: Delta,
}
//--- ---//

//--- CLIENT STUFF ---//
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientSettings {
    pub local_port: u16,
    pub server_port: u16,
    pub host_name: String,
    pub host_address: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LinkType {
    Bijection,
    Injection,
    BlockInjection,
}
//--- ---//
