use bbup_rust::fs;
use bbup_rust::fstree::{DeltaFSTree, FSTree};
use bbup_rust::model::Commit;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use anyhow::Result;

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
                output.merge_prec(&delta_at_endpoint)?;
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

// TODO restructure ServerState in a similar way to how ClientState is structured
pub struct ServerState {
    pub archive_root: PathBuf,
    pub server_port: u16,
    pub commit_list: CommitList,
    pub archive_tree: FSTree,
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
        let archive_tree: FSTree = fs::load(&archive_root.join(".bbup").join("archive-tree.json"))?;
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
