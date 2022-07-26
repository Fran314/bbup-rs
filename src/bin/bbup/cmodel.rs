use serde::{Deserialize, Serialize};

use bbup_rust::{
    fs,
    fstree::{DeltaFSTree, FSTree},
    model::{Commit, ExcludeList},
};

use std::path::PathBuf;

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
    pub link_root: PathBuf,
    pub exclude_list: ExcludeList,
    pub endpoint: Vec<String>,
    pub connection: Connection,
    pub flags: Flags,
}
pub struct ProcessState {
    pub last_known_commit: String,
    pub last_known_fstree: FSTree,
    pub new_tree: Option<FSTree>,
    pub local_delta: Option<DeltaFSTree>,
    pub update: Option<Commit>,
}
impl ProcessState {
    fn lkc_path(link_root: &PathBuf) -> PathBuf {
        link_root.join(".bbup").join("last-known-commit.bin")
    }
    fn ofst(link_root: &PathBuf) -> PathBuf {
        link_root.join(".bbup").join("old-fstree.bin")
    }
    pub fn from(lkc: String, last_known_fstree: FSTree) -> ProcessState {
        ProcessState {
            last_known_commit: lkc,
            last_known_fstree,
            new_tree: None,
            local_delta: None,
            update: None,
        }
    }
    pub fn load(link_root: &PathBuf) -> Result<ProcessState> {
        let lkc: String = fs::load(ProcessState::lkc_path(link_root))
            .context("failed to load link's last known commit")?;
        let last_known_fstree: FSTree =
            fs::load(ProcessState::ofst(link_root)).context("failed to load link's old fstree")?;

        Ok(ProcessState::from(lkc, last_known_fstree))
    }
    pub fn save(&self, link_root: &PathBuf) -> Result<()> {
        fs::save(&ProcessState::lkc_path(link_root), &self.last_known_commit)
            .context("failed to save link's last known commit")?;
        fs::save(&ProcessState::ofst(link_root), &self.last_known_fstree)
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
    fn path(home_dir: &PathBuf) -> PathBuf {
        home_dir
            .join(".config")
            .join("bbup-client")
            .join("config.toml")
    }
    pub fn exists(home_dir: &PathBuf) -> bool {
        ClientConfig::path(home_dir).exists()
    }
    pub fn from(settings: ClientSettings, links: Vec<String>) -> ClientConfig {
        ClientConfig { settings, links }
    }
    pub fn load(home_dir: &PathBuf) -> Result<ClientConfig> {
        let path = ClientConfig::path(home_dir);
        if !path.exists() {
            anyhow::bail!("Bbup client isn't set up. Try using 'bbup setup'")
        }
        let client_config: ClientConfig =
            fs::load(&path).context("failed to laod client config")?;
        Ok(client_config)
    }
    pub fn save(&self, home_dir: &PathBuf) -> Result<()> {
        fs::save(&ClientConfig::path(home_dir), &self).context("failed to save client config")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkConfig {
    pub link_type: LinkType,
    pub endpoint: Vec<String>,
    pub exclude_list: Vec<String>,
}
impl LinkConfig {
    fn path(link_root: &PathBuf) -> PathBuf {
        link_root.join(".bbup").join("config.toml")
    }
    pub fn exists(link_root: &PathBuf) -> bool {
        LinkConfig::path(link_root).exists()
    }
    pub fn from(
        link_type: LinkType,
        endpoint: Vec<String>,
        exclude_list: Vec<String>,
    ) -> LinkConfig {
        LinkConfig {
            link_type,
            endpoint,
            exclude_list,
        }
    }
    pub fn load(link_root: &PathBuf) -> Result<LinkConfig> {
        let path = LinkConfig::path(link_root);
        if !path.exists() {
            anyhow::bail!(
                "Current directory [{:?}] isn't initialized as a backup source",
                link_root
            )
        }
        let link_config: LinkConfig = fs::load(&path).context("failed to load link config")?;
        Ok(link_config)
    }
    pub fn save(&self, link_root: &PathBuf) -> Result<()> {
        fs::save(&LinkConfig::path(link_root), &self).context("failed to save link config")?;
        Ok(())
    }
}
