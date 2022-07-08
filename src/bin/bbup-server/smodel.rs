use bbup_rust::fs;
use bbup_rust::hashtree::Tree;
use bbup_rust::model::{Change, Commit, Delta, DeltaExt};
use bbup_rust::path::AbstractPath;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use anyhow::Result;

pub type CommitList = Vec<Commit>;
pub trait CommmitListExt {
    fn get_update_delta(&self, endpoint: &AbstractPath, lkc: String) -> Delta;
}
impl CommmitListExt for CommitList {
    fn get_update_delta(&self, endpoint: &AbstractPath, lkc: String) -> Delta {
        let mut output: Delta = Vec::new();
        for commit in self.into_iter().rev() {
            if commit.commit_id.eq(&lkc) {
                break;
            }
            output.merge_delta(&commit.delta);
        }
        output
            .iter()
            .filter_map(|change| match change.path.strip_prefix(endpoint) {
                Ok(val) => Some(Change {
                    path: val,
                    change_type: change.change_type.clone(),
                }),
                Err(_) => None,
            })
            .collect()
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub server_port: u16,
    pub archive_root: PathBuf,
}

// TODO restructure ServerState in a similar way to how ClientState is structured
pub struct ServerState {
    pub archive_root: PathBuf,
    pub server_port: u16,
    pub commit_list: CommitList,
    pub archive_tree: Tree,
}

impl ServerState {
    pub fn load(home_dir: PathBuf) -> Result<ServerState> {
        let config: ServerConfig = fs::load(
            &home_dir
                .join(".config")
                .join("bbup-server")
                .join("config.yaml"),
        )?;

        let archive_root = home_dir.join(config.archive_root);

        // Load server state, necessary for conversation and
        //	"shared" between tasks (though only one can use it
        //	at a time and those who can't have it terminate)
        let commit_list: CommitList =
            fs::load(&archive_root.join(".bbup").join("commit-list.json"))?;
        let archive_tree: Tree = fs::load(&archive_root.join(".bbup").join("archive-tree.json"))?;
        Ok(ServerState {
            archive_root,
            server_port: config.server_port,
            commit_list,
            archive_tree,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        fs::save(
            &self.archive_root.join(".bbup").join("commit-list.json"),
            &self.commit_list,
        )?;
        fs::save(
            &self.archive_root.join(".bbup").join("archive-tree.json"),
            &self.archive_tree,
        )?;

        Ok(())
    }
}
