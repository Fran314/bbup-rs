use std::collections::HashMap;

use abst_fs::{self as fs, AbstPath};
use fs_vcs::{Commit, CommitID, CommitList, Delta, FSTree, GetUpdError, InapplicableDelta};

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

    pub fn endpoint_root(archive_root: &AbstPath, endpoint: impl ToString) -> AbstPath {
        archive_root
            .add_last("endpoints")
            .append(&AbstPath::from(endpoint.to_string()))
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
    pub fn get_mut(&mut self, endpoint: impl ToString) -> Option<&mut ArchiveEndpoint> {
        self.0.get_mut(&endpoint.to_string())
    }

    pub fn load(archive_root: &AbstPath) -> Result<Archive> {
        let Keys { list }: Keys = fs::load(&archive_root.add_last("archive-endpoints.toml"))
            .context("failed to load list of archive endpoints")?;

        let mut archive = Archive::new();
        for name in list {
            let endpoint_root = ArchiveEndpoint::endpoint_root(archive_root, &name);
            archive.insert(
                name.clone(),
                ArchiveEndpoint::load(&endpoint_root)
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
}

/// Convert hash object to path, distributing the path on 4 subdirectories
/// to avoid having too many objects in one directory
///
/// The hash object represented by the string
///     8f453d680c1a1afe3679139be91242ee3ea904f6aa66b3bc2fcae7c469fea2c5
/// would get converted to the path
///     8f/45/3d/68/0c1a1afe3679139be91242ee3ea904f6aa66b3bc2fcae7c469fea2c5
pub fn hash_to_path(hash: hasher::Hash) -> AbstPath {
    const FRAGMENTATION_DEPTH: usize = 4;

    let s = hash.to_string();
    let mut output = AbstPath::empty();
    for i in 0..FRAGMENTATION_DEPTH {
        let fragment = s[(i * 2)..(i * 2 + 2)].to_string();
        output = output.add_last(fragment);
    }
    let last_fragment = s[(FRAGMENTATION_DEPTH * 2)..].to_string();
    output = output.add_last(last_fragment);

    output
}
