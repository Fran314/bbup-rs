use clap::Parser;
use serde::{Deserialize, Serialize};

use abst_fs::{self as fs, AbstPath};
use fs_vcs::{CommitID, ExcludeList, FSTree};

use anyhow::{Context, Result};

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
    pub link_root: AbstPath,
    pub exclude_list: ExcludeList,
    pub endpoint: String,
    pub connection: Connection,
    pub flags: Flags,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientConfig {
    pub links: Vec<String>,
    pub settings: ClientSettings,
}
impl ClientConfig {
    fn path(home_dir: &AbstPath) -> AbstPath {
        home_dir
            .add_last(".config")
            .add_last("bbup-client")
            .add_last("config.toml")
    }
    pub fn exists(home_dir: &AbstPath) -> bool {
        ClientConfig::path(home_dir).exists()
    }
    pub fn from(settings: ClientSettings, links: Vec<String>) -> ClientConfig {
        ClientConfig { settings, links }
    }
    pub fn load(home_dir: &AbstPath) -> Result<ClientConfig> {
        let path = ClientConfig::path(home_dir);
        if !path.exists() {
            anyhow::bail!("Bbup client isn't set up. Try using 'bbup setup'")
        }
        let client_config: ClientConfig =
            fs::load(&path).context("failed to laod client config")?;
        Ok(client_config)
    }
    pub fn save(&self, home_dir: &AbstPath) -> Result<()> {
        fs::save(&ClientConfig::path(home_dir), &self).context("failed to save client config")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkConfig {
    pub link_type: LinkType,
    pub endpoint: String,
    pub exclude_list: Vec<String>,
}
impl LinkConfig {
    fn path(link_root: &AbstPath) -> AbstPath {
        link_root.add_last(".bbup").add_last("config.toml")
    }
    pub fn exists(link_root: &AbstPath) -> bool {
        LinkConfig::path(link_root).exists()
    }
    pub fn from(link_type: LinkType, endpoint: String, exclude_list: Vec<String>) -> LinkConfig {
        LinkConfig {
            link_type,
            endpoint,
            exclude_list,
        }
    }
    pub fn load(link_root: &AbstPath) -> Result<LinkConfig> {
        let path = LinkConfig::path(link_root);
        if !path.exists() {
            anyhow::bail!("Current directory [{link_root}] isn't initialized as a backup source")
        }
        let link_config: LinkConfig = fs::load(&path).context("failed to load link config")?;
        Ok(link_config)
    }
    pub fn save(&self, link_root: &AbstPath) -> Result<()> {
        fs::save(&LinkConfig::path(link_root), &self).context("failed to save link config")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct LinkState {
    pub last_known_commit: CommitID,
    pub last_known_fstree: FSTree,
}
impl LinkState {
    pub fn from(lkc: CommitID, last_known_fstree: FSTree) -> LinkState {
        LinkState {
            last_known_commit: lkc,
            last_known_fstree,
        }
    }
    pub fn init_state() -> LinkState {
        LinkState::from(CommitID::gen_null(), FSTree::new())
    }
    fn path(link_root: &AbstPath) -> AbstPath {
        link_root.add_last(".bbup").add_last("state.bin")
    }
    pub fn load(link_root: &AbstPath) -> Result<LinkState> {
        let state: LinkState =
            fs::load(&LinkState::path(link_root)).context("failed to load state")?;
        Ok(state)
    }
    pub fn save(&self, link_root: &AbstPath) -> Result<()> {
        fs::save(&LinkState::path(link_root), &self).context("failed to save state")?;
        Ok(())
    }
}

// Program options
#[derive(Parser, Debug, PartialEq)]
pub struct BackOps {
    /// Increase verbosity
    #[clap(short, long, value_parser)]
    pub verbose: bool,

    /// Show progress during file transfer
    #[clap(short, long, value_parser)]
    pub progress: bool,
}

#[derive(Parser, Debug, PartialEq)]
pub struct InitOps {
    /// Set endpoint
    #[clap(short, long)]
    pub endpoint: Option<String>,

    /// Set exclude list to empty
    #[clap(short, long)]
    pub no_exclude_list: bool,
}

#[derive(Parser, Debug, PartialEq)]
pub struct SetupOps {
    /// Set port for client
    #[clap(short, long, value_parser)]
    pub local_port: Option<u16>,

    /// Set port for server
    #[clap(short, long, value_parser)]
    pub server_port: Option<u16>,

    /// Set server username
    #[clap(short = 'n', long, value_parser)]
    pub host_name: Option<String>,

    /// Set server address
    #[clap(short = 'a', long, value_parser)]
    pub host_address: Option<String>,
}
