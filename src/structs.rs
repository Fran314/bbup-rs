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
#[derive(Serialize, Deserialize, Debug)]
pub struct Commit {
    pub commit_id: String,
    pub delta: Delta,
}
//--- ---//

//--- CLIENT STUFF ---//
#[derive(Serialize, Deserialize, Debug)]
pub struct ClientSettings {
    pub local_port: u16,
    pub server_port: u16,
    pub host_name: String,
    pub host_address: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum LinkType {
    Bijection,
    Injection,
    BlockInjection,
}
//--- ---//
