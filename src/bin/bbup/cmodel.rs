use serde::{Deserialize, Serialize};

use bbup_rust::{
    fstree::{DeltaFSTree, FSTree},
    model::{Commit, ExcludeList},
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
    pub endpoint: Vec<String>,
    pub connection: Connection,
    pub flags: Flags,
}
pub struct ProcessState {
    pub last_known_commit: Option<String>,
    pub old_tree: Option<FSTree>,
    pub new_tree: Option<FSTree>,
    pub local_delta: Option<DeltaFSTree>,
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
    pub endpoint: Vec<String>,
    pub exclude_list: Vec<String>,
}
