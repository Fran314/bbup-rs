use std::collections::HashMap;

use abst_fs::{self as fs, AbstPath};
use fs_vcs::{Commit, CommitID, CommitList, Delta, FSTree, GetUpdError, InapplicableDelta};

use serde::{Deserialize, Serialize};

use anyhow::{bail, Context, Result};

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

#[derive(Serialize, Deserialize, Clone)]
pub struct ArchiveEndpoint {
    state: FSTree,
    history: CommitList,
}
impl ArchiveEndpoint {
    pub fn new() -> ArchiveEndpoint {
        ArchiveEndpoint {
            state: FSTree::new(),
            history: CommitList::base_commit_list(),
        }
    }
    pub fn most_recent_commit(&self) -> &Commit {
        self.history.most_recent_commit()
    }

    pub fn get_update_delta(&self, lkc: CommitID) -> Result<Delta, GetUpdError> {
        self.history.get_update_delta(lkc)
    }
    pub fn is_delta_applicable(&self, delta: &Delta) -> Result<(), InapplicableDelta> {
        let mut state_copy = self.state.clone();
        state_copy.apply_delta(delta)
    }
    pub fn commit_delta(&mut self, delta: &Delta) -> Result<CommitID, InapplicableDelta> {
        self.state.apply_delta(delta)?;
        let id = CommitID::gen_valid();
        self.history.push(Commit {
            commit_id: id.clone(),
            delta: delta.clone(),
        });

        Ok(id)
    }

    pub fn load(path: &AbstPath) -> Result<ArchiveEndpoint> {
        let state: FSTree =
            fs::load(&path.add_last("state.bin")).context("failed to load state")?;
        let history: CommitList =
            fs::load(&path.add_last("history.bin")).context("failed to load history")?;
        Ok(ArchiveEndpoint { state, history })
    }
    pub fn save(&self, path: &AbstPath) -> Result<()> {
        fs::save(&path.add_last("state.bin"), &self.state).context("failed to save state")?;
        fs::save(&path.add_last("history.bin"), &self.history).context("failed to save history")?;
        Ok(())
    }
}

/// This Keys struct is only needed to serialize Archive keys to .toml, because
/// otherwise it might try to serialize an empty list to .toml which is just a
/// []
/// which is invalid toml.
/// This way, it gets serialzied to
/// list = []
/// which can be unwrapped to the desired result
#[derive(Serialize, Deserialize)]
struct Keys {
    list: Vec<String>,
}
pub struct Archive(HashMap<String, ArchiveEndpoint>);
#[allow(clippy::new_without_default)]
impl Archive {
    pub fn new() -> Archive {
        Archive(HashMap::new())
    }

    pub fn insert(&mut self, key: String, endpoint: ArchiveEndpoint) -> Option<ArchiveEndpoint> {
        self.0.insert(key, endpoint)
    }
    pub fn get(&mut self, endpoint: impl ToString) -> Option<&ArchiveEndpoint> {
        self.0.get(&endpoint.to_string())
    }
    pub fn get_mut(&mut self, endpoint: impl ToString) -> Option<&mut ArchiveEndpoint> {
        self.0.get_mut(&endpoint.to_string())
    }

    pub fn load(archive_root: &AbstPath) -> Result<Archive> {
        let Keys { list }: Keys = fs::load(&archive_root.add_last("archive-endpoints.toml"))
            .context("failed to load list of archive endpoints")?;

        let mut archive = Archive::new();
        for name in list {
            archive.insert(
                name.clone(),
                ArchiveEndpoint::load(&archive_root.add_last(&name))
                    .context(format!("failed to load endpoint {name}"))?,
            );
        }

        Ok(archive)
    }
    pub fn save_list(&self, archive_root: &AbstPath) -> Result<()> {
        let keys = Keys {
            list: self.0.keys().into_iter().cloned().collect(),
        };
        fs::save(&archive_root.add_last("archive-endpoints.toml"), &keys)
            .context("failed to save list of archive endpoints")?;

        Ok(())
    }
    pub fn save_endpoint(&self, archive_root: &AbstPath, name: String) -> Result<()> {
        match self.0.get(&name) {
            Some(endpoint) => {
                endpoint
                    .save(&archive_root.add_last(&name))
                    .context(format!("failed to save endpoint {name}"))?;

                Ok(())
            }
            None => {
                bail!("failed to save endpoint {name}, it does not exist!")
            }
        }
    }
}

// impl ArchiveEndpoint {}
// pub struct NewArchiveState {
//     pub archive_endpoints: Archive,
// }
// impl NewArchiveState {
//     pub fn load_all(archive_root: &AbstPath) -> Result<NewArchiveState> {
//         let list: Vec<String> = fs::load(&archive_root.add_last("archive-endpoints.toml"))
//             .context("failed to load list of archive endpoints")?;
//
//         let mut archive_endpoints = Archive::new();
//         for endpoint in list {
//             let state: FSTree = fs::load(&archive_root.add_last(&endpoint).add_last("state.bin"))
//                 .context(format!("failed to load state of endpoint {endpoint}"))?;
//             let history: CommitList =
//                 fs::load(&archive_root.add_last(&endpoint).add_last("history.bin")).context(
//                     format!("failed to load load history of endpoint {endpoint}"),
//                 )?;
//             archive_endpoints.insert(endpoint, ArchiveEndpoint { state, history });
//         }
//
//         Ok(NewArchiveState { archive_endpoints })
//     }
//
//     pub fn save_list(&self, archive_root: &AbstPath) -> Result<()> {
//         fs::save(
//             &archive_root.add_last("archive-endpoints.toml"),
//             &self.archive_endpoints.keys(),
//         )
//         .context("failed to save list of archive endpoints")?;
//
//         Ok(())
//     }
//     pub fn save_endpoint(&self, archive_root: &AbstPath, endpoint: String) -> Result<()> {
//         match self.archive_endpoints.get(endpoint) {
//             Some(ArchiveEndpoint { state, history }) => {
//                 fs::save(
//                     &archive_root.add_last(&endpoint).add_last("state.bin"),
//                     state,
//                 )
//                 .context(format!("failed to save state of endpoint {endpoint}"))?;
//
//                 fs::save(
//                     &archive_root.add_last(&endpoint).add_last("history.bin"),
//                     history,
//                 )
//                 .context(format!("failed to save history of endpoint {endpoint}"))?;
//
//                 Ok(())
//             }
//             None => {
//                 bail!("failed to save endpoint {endpoint}, it does not exist!")
//             }
//         }
//     }
// }

// #[derive(Serialize, Deserialize, Debug, Clone)]
// pub struct ArchiveConfig {
//     pub archive_root: AbstPath,
// }
//
// #[derive(Debug)]
// pub struct ArchiveState {
//     pub commit_list: CommitList,
//     pub archive_tree: FSTree,
// }
// impl ArchiveState {
//     pub fn from(commit_list: CommitList, archive_tree: FSTree) -> ArchiveState {
//         ArchiveState {
//             commit_list,
//             archive_tree,
//         }
//     }
//     pub fn init_state() -> ArchiveState {
//         ArchiveState::from(CommitList::base_commit_list(), FSTree::new())
//     }
//     fn cl_path(archive_root: &AbstPath) -> AbstPath {
//         archive_root.add_last(".bbup").add_last("commit-list.bin")
//     }
//     fn at_path(archive_root: &AbstPath) -> AbstPath {
//         archive_root.add_last(".bbup").add_last("archive-tree.bin")
//     }
//     pub fn load(archive_root: &AbstPath) -> Result<ArchiveState> {
//         let commit_list: CommitList = fs::load(&ArchiveState::cl_path(archive_root))
//             .context("failed to load archive's commit list")?;
//
//         let archive_tree: FSTree = fs::load(&ArchiveState::at_path(archive_root))
//             .context("failed to load archive's tree")?;
//
//         Ok(ArchiveState {
//             commit_list,
//             archive_tree,
//         })
//     }
//     pub fn save(&self, archive_root: &AbstPath) -> Result<()> {
//         fs::save(&ArchiveState::cl_path(archive_root), &self.commit_list)
//             .context("failed to save archive's commit list")?;
//
//         fs::save(&ArchiveState::at_path(archive_root), &self.archive_tree)
//             .context("failed to save archive's tree")?;
//
// pub fn init_state() -> NewArchiveState {
//     NewArchiveState {
//         archive_endpoints: Archive::new(),
//     }
// }
//         Ok(())
//     }
// }

/// Convert hash object to path, distributing the path on 4 subdirectories
/// to avoid having too many objects in one directory
///
/// The hash object represented by the string
///     8f453d680c1a1afe3679139be91242ee3ea904f6aa66b3bc2fcae7c469fea2c5
/// would get converted to the path
///     8f/45/3d/68/0c1a1afe3679139be91242ee3ea904f6aa66b3bc2fcae7c469fea2c5
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
