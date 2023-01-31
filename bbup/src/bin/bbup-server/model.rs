use abst_fs::{self as fs, AbstPath};
use fs_vcs::{CommitList, FSTree};

use serde::{Deserialize, Serialize};

use anyhow::{Context, Result};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub server_port: u16,
    pub archive_root: AbstPath,
}
impl ServerConfig {
    fn path(home_dir: &AbstPath) -> AbstPath {
        home_dir
            .add_last(".config")
            .add_last("bbup-server")
            .add_last("config.toml")
    }
    pub fn from(server_port: u16, archive_root: AbstPath) -> ServerConfig {
        ServerConfig {
            server_port,
            archive_root,
        }
    }
    pub fn exists(home_dir: &AbstPath) -> bool {
        ServerConfig::path(home_dir).exists()
    }
    pub fn load(home_dir: &AbstPath) -> Result<ServerConfig> {
        let path = ServerConfig::path(home_dir);
        if !path.exists() {
            anyhow::bail!("Bbup server isn't set up. Try using 'bbup-server setup'")
        }
        let server_config: ServerConfig =
            fs::load(&path).context("failed to load server config")?;
        Ok(server_config)
    }
    pub fn save(&self, home_dir: &AbstPath) -> Result<()> {
        fs::save(&ServerConfig::path(home_dir), self).context("failed to save server config")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ArchiveConfig {
    pub archive_root: AbstPath,
}

#[derive(Debug)]
pub struct ArchiveState {
    pub commit_list: CommitList,
    pub archive_tree: FSTree,
}
impl ArchiveState {
    pub fn from(commit_list: CommitList, archive_tree: FSTree) -> ArchiveState {
        ArchiveState {
            commit_list,
            archive_tree,
        }
    }
    pub fn init_state() -> ArchiveState {
        ArchiveState::from(CommitList::base_commit_list(), FSTree::new())
    }
    fn cl_path(archive_root: &AbstPath) -> AbstPath {
        archive_root.add_last(".bbup").add_last("commit-list.bin")
    }
    fn at_path(archive_root: &AbstPath) -> AbstPath {
        archive_root.add_last(".bbup").add_last("archive-tree.bin")
    }
    pub fn load(archive_root: &AbstPath) -> Result<ArchiveState> {
        let commit_list: CommitList = fs::load(&ArchiveState::cl_path(archive_root))
            .context("failed to load archive's commit list")?;

        let archive_tree: FSTree = fs::load(&ArchiveState::at_path(archive_root))
            .context("failed to load archive's tree")?;

        Ok(ArchiveState {
            commit_list,
            archive_tree,
        })
    }
    pub fn save(&self, archive_root: &AbstPath) -> Result<()> {
        fs::save(&ArchiveState::cl_path(archive_root), &self.commit_list)
            .context("failed to save archive's commit list")?;

        fs::save(&ArchiveState::at_path(archive_root), &self.archive_tree)
            .context("failed to save archive's tree")?;

        Ok(())
    }
}

pub fn hash_to_path(hash: hasher::Hash) -> AbstPath {
    let s = hash.to_string();
    AbstPath::from(format!(
        "{}/{}/{}/{}/{}",
        &s[..2],
        &s[2..4],
        &s[4..6],
        &s[6..8],
        &s[8..]
    ))
}
