use serde::{Deserialize, Serialize};
use std::path::PathBuf;

//--- COMMON STUFF ---//
#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone)]
pub enum ObjectType {
    Dir,
    File,
    Symlink,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone)]
pub enum Action {
    Added,
    Edited,
    Removed,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Change {
    pub action: Action,
    pub object_type: ObjectType,
    pub path: PathBuf,
    pub hash: Option<String>,
}
impl Change {
    pub fn new(
        action: Action,
        object_type: ObjectType,
        path: PathBuf,
        hash: Option<String>,
    ) -> Change {
        Change {
            action,
            object_type,
            path,
            hash,
        }
    }
}
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
            output += match self[i].action {
                Action::Added => "+++  ",
                Action::Edited => "~~~  ",
                Action::Removed => "---  ",
            };
            output += match self[i].object_type {
                ObjectType::Dir => "dir   ",
                ObjectType::File => "file  ",
                ObjectType::Symlink => "sylk  ",
            };
            match self[i].path.to_str() {
                Some(val) => output += val,
                None => output += format!("[non-utf8 path] {:?}", self[i].path).as_str(),
            };
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

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateRequest {
    pub endpoint: PathBuf,
    pub lkc: String,
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
