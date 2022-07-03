use serde::{Deserialize, Serialize};

use bbup_rust::{
    hashtree::Tree,
    path::AbstractPath,
    structs::{Commit, Delta, ExcludeList},
};

use std::path::PathBuf;

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

pub struct Flags {
    pub verbose: bool,
    pub progress: bool,
}
pub struct Connection {
    pub local_port: u16,
    pub server_port: u16,
    pub host_name: String,
    pub host_address: String,
}
pub struct ProcessConfig {
    pub link_root: PathBuf,
    pub exclude_list: ExcludeList,
    pub endpoint: AbstractPath,
    pub connection: Connection,
    pub flags: Flags,
}
impl ProcessConfig {
    pub fn local_temp_path(&self) -> PathBuf {
        self.link_root.join(".bbup").join("temp")
    }
    pub fn lkc_path(&self) -> PathBuf {
        self.link_root.join(".bbup").join("last-known-commit.json")
    }
    pub fn old_tree_path(&self) -> PathBuf {
        self.link_root.join(".bbup").join("old-hash-tree.json")
    }
}
pub struct ProcessState {
    pub last_known_commit: Option<String>,
    pub old_tree: Option<Tree>,
    pub new_tree: Option<Tree>,
    pub local_delta: Option<Delta>,
    pub update: Option<Commit>,
}
impl ProcessState {
    pub fn new() -> ProcessState {
        ProcessState {
            last_known_commit: None,
            old_tree: None,
            new_tree: None,
            local_delta: None,
            update: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientConfig {
    pub settings: ClientSettings,
    pub links: Vec<String>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkConfig {
    pub link_type: LinkType,
    pub endpoint: AbstractPath,
    pub exclude_list: Vec<String>,
}
