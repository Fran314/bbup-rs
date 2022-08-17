use std::collections::HashMap;

use bbup_rust::fs::{self, AbstPath};
use bbup_rust::fstree::{Delta, DeltaNode, FSTree};
use bbup_rust::model::Commit;

use serde::{Deserialize, Serialize};

use anyhow::{Context, Result};

pub type CommitList = Vec<Commit>;
pub trait CommmitListExt {
    fn get_update_delta(&self, endpoint: &AbstPath, lkc: String) -> Result<Delta>;
}
impl CommmitListExt for CommitList {
    fn get_update_delta(&self, endpoint: &AbstPath, lkc: String) -> Result<Delta> {
        let mut output: Delta = Delta::empty();
        'commit_loop: for commit in self.iter().rev() {
            if commit.commit_id.eq(&lkc) {
                break;
            }
            let mut delta = commit.delta.clone();
            let mut commit_endpoint = commit.endpoint.clone();
            let mut curr_endpoint = endpoint.clone();

            for component in endpoint {
                match commit_endpoint.get(0) {
                    Some(comp) if component == comp => {
                        commit_endpoint = commit_endpoint.strip_first();
                        curr_endpoint = curr_endpoint.strip_first();
                    }
                    Some(_) => continue 'commit_loop,
                    None => break,
                }
            }
            for component in commit_endpoint.into_iter().rev() {
                let node = DeltaNode::Branch(None, delta);
                let tree = HashMap::from([(component, node)]);
                delta = Delta(tree)
            }

            if let Some(delta_at_endpoint) = delta.get_subdelta_tree_copy(&curr_endpoint) {
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
    fn cl_path(archive_root: &AbstPath) -> AbstPath {
        archive_root.add_last(".bbup").add_last("commit-list.bin")
    }
    fn at_path(archive_root: &AbstPath) -> AbstPath {
        archive_root.add_last(".bbup").add_last("archive-tree.bin")
    }
    pub fn from(commit_list: CommitList, archive_tree: FSTree) -> ArchiveState {
        ArchiveState {
            commit_list,
            archive_tree,
        }
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
