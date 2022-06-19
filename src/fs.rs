use crate::utils;

use std::fs;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
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

//--- SERVER STUFF ---//
pub type CommitList = Vec<Commit>;
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
pub struct ClientConfig {
    pub settings: ClientSettings,
    pub links: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum LinkType {
    Bijection,
    Injection,
    BlockInjection,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct LinkConfig {
    pub link_type: LinkType,
    pub exclude_list: Vec<String>,
}
//--- ---//

pub fn load_json<T: DeserializeOwned>(path: &PathBuf) -> std::io::Result<T> {
    let serialized = fs::read_to_string(path)?;
    let content: T = serde_json::from_str(&serialized)?;
    Ok(content)
}

pub fn save_json<T: Serialize>(path: &PathBuf, content: &T) -> std::io::Result<()> {
    let serialized = serde_json::to_string(content)?;
    fs::write(path, serialized)?;
    Ok(())
}

pub fn load_yaml<T: DeserializeOwned>(path: &PathBuf) -> std::io::Result<T> {
    let serialized = fs::read_to_string(&path)?;
    let content: T = serde_yaml::from_str(&serialized).map_err(utils::to_io_err)?;
    Ok(content)
}

pub fn save_yaml<T: Serialize>(path: &PathBuf, content: &T) -> std::io::Result<()> {
    let serialized = serde_yaml::to_string(content).map_err(utils::to_io_err)?;
    fs::write(path, serialized)?;
    Ok(())
}
