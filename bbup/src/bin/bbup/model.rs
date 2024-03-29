use serde::{Deserialize, Serialize};

use abst_fs::{self as fs, AbstPath};
use fs_vcs::{Commit, Delta, ExcludeList, FSTree};

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
    pub endpoint: AbstPath,
    pub connection: Connection,
    pub flags: Flags,
}
pub struct ProcessState {
    pub last_known_commit: String,
    pub last_known_fstree: FSTree,
    pub new_tree: Option<FSTree>,
    pub local_delta: Option<Delta>,
    pub update: Option<(String, Delta)>,
}
impl ProcessState {
    pub fn from(lkc: String, last_known_fstree: FSTree) -> ProcessState {
        ProcessState {
            last_known_commit: lkc,
            last_known_fstree,
            new_tree: None,
            local_delta: None,
            update: None,
        }
    }
    pub fn init_state() -> ProcessState {
        ProcessState::from(Commit::base_commit().commit_id, FSTree::empty())
    }
    fn lkc_path(link_root: &AbstPath) -> AbstPath {
        link_root
            .add_last(".bbup")
            .add_last("last-known-commit.bin")
    }
    fn ofst_path(link_root: &AbstPath) -> AbstPath {
        link_root.add_last(".bbup").add_last("old-fstree.bin")
    }
    pub fn load(link_root: &AbstPath) -> Result<ProcessState> {
        let lkc: String = fs::load(&ProcessState::lkc_path(link_root))
            .context("failed to load link's last known commit")?;
        let last_known_fstree: FSTree = fs::load(&ProcessState::ofst_path(link_root))
            .context("failed to load link's old fstree")?;

        Ok(ProcessState::from(lkc, last_known_fstree))
    }
    pub fn save(&self, link_root: &AbstPath) -> Result<()> {
        fs::save(&ProcessState::lkc_path(link_root), &self.last_known_commit)
            .context("failed to save link's last known commit")?;
        fs::save(&ProcessState::ofst_path(link_root), &self.last_known_fstree)
            .context("failed to save link's old fstree")?;

        Ok(())
    }
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
    pub endpoint: AbstPath,
    pub exclude_list: Vec<String>,
}
impl LinkConfig {
    fn path(link_root: &AbstPath) -> AbstPath {
        link_root.add_last(".bbup").add_last("config.toml")
    }
    pub fn exists(link_root: &AbstPath) -> bool {
        LinkConfig::path(link_root).exists()
    }
    pub fn from(link_type: LinkType, endpoint: AbstPath, exclude_list: Vec<String>) -> LinkConfig {
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
