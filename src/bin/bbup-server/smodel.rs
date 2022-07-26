use bbup_rust::fs;
use bbup_rust::fstree::{DeltaFSTree, FSTree};
use bbup_rust::model::Commit;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use anyhow::{Context, Result};

pub type CommitList = Vec<Commit>;
pub trait CommmitListExt {
    fn get_update_delta(&self, endpoint: &Vec<String>, lkc: String) -> Result<DeltaFSTree>;
}
impl CommmitListExt for CommitList {
    fn get_update_delta(&self, endpoint: &Vec<String>, lkc: String) -> Result<DeltaFSTree> {
        let mut output: DeltaFSTree = DeltaFSTree::empty();
        for commit in self.into_iter().rev() {
            if commit.commit_id.eq(&lkc) {
                break;
            }
            if let Some(delta_at_endpoint) = commit.delta.get_subdelta_tree_copy(endpoint) {
                output.merge_prec(&delta_at_endpoint).context(format!(
                    "failed to merge commit {} with successive commits",
                    commit.commit_id
                ))?;
            }
        }
        Ok(output)
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub server_port: u16,
    pub archive_root: PathBuf,
}
impl ServerConfig {
    fn path(home_dir: &PathBuf) -> PathBuf {
        home_dir
            .join(".config")
            .join("bbup-server")
            .join("config.toml")
    }
    pub fn from(server_port: u16, archive_root: PathBuf) -> ServerConfig {
        ServerConfig {
            server_port,
            archive_root,
        }
    }
    pub fn exists(home_dir: &PathBuf) -> bool {
        ServerConfig::path(home_dir).exists()
    }
    pub fn load(home_dir: &PathBuf) -> Result<ServerConfig> {
        let path = ServerConfig::path(home_dir);
        if !path.exists() {
            anyhow::bail!("Bbup server isn't set up. Try using 'bbup-server setup'")
        }
        let server_config: ServerConfig =
            fs::load(&path).context("failed to load server config")?;
        Ok(server_config)
    }
    pub fn save(&self, home_dir: &PathBuf) -> Result<()> {
        fs::save(ServerConfig::path(home_dir), self).context("failed to save server config")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ArchiveConfig {
    pub archive_root: PathBuf,
}

#[derive(Debug)]
pub struct ArchiveState {
    pub commit_list: CommitList,
    pub archive_tree: FSTree,
}
impl ArchiveState {
    fn cl_path(archive_root: &PathBuf) -> PathBuf {
        archive_root.join(".bbup").join("commit-list.bin")
    }
    fn at_path(archive_root: &PathBuf) -> PathBuf {
        archive_root.join(".bbup").join("archive-tree.bin")
    }
    pub fn from(commit_list: CommitList, archive_tree: FSTree) -> ArchiveState {
        ArchiveState {
            commit_list,
            archive_tree,
        }
    }
    pub fn load(archive_root: &PathBuf) -> Result<ArchiveState> {
        let commit_list: CommitList = fs::load(&ArchiveState::cl_path(archive_root))
            .context("failed to load archive's commit list")?;

        let archive_tree: FSTree = fs::load(&ArchiveState::at_path(archive_root))
            .context("failed to load archive's tree")?;

        Ok(ArchiveState {
            commit_list,
            archive_tree,
        })
    }
    pub fn save(&self, archive_root: &PathBuf) -> Result<()> {
        fs::save(&ArchiveState::cl_path(archive_root), &self.commit_list)
            .context("failed to save archive's commit list")?;

        fs::save(&ArchiveState::at_path(archive_root), &self.archive_tree)
            .context("failed to save archive's tree")?;

        Ok(())
    }
}
