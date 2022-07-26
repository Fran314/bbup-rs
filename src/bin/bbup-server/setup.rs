use crate::smodel::ArchiveState;
use crate::ServerConfig;

use bbup_rust::fstree::FSTree;
use bbup_rust::input;
use bbup_rust::model::Commit;

use std::path::PathBuf;

use anyhow::Result;

pub fn setup(home_dir: PathBuf) -> Result<()> {
    if ServerConfig::exists(&home_dir) {
        anyhow::bail!("bbup server is already setup");
    }

    let server_port = input::get("enter server port (0-65535): ")?.parse::<u16>()?;
    let archive_root = PathBuf::from(input::get("enter archive root (relative to ~): ")?);
    if !home_dir.join(&archive_root).exists() {
        anyhow::bail!("specified archive root does not exist!");
    }
    if !home_dir.join(&archive_root).is_dir() {
        anyhow::bail!("specified archive root is not a directory!");
    }
    // TODO maybe make so that an archive can be setup from a non-empty state
    if !home_dir.join(&archive_root).read_dir()?.next().is_none() {
        anyhow::bail!("specified archive root is not empty!");
    }

    ServerConfig::from(server_port, archive_root.clone()).save(&home_dir)?;

    let archive_root = home_dir.join(archive_root);
    ArchiveState::from(vec![Commit::base_commit()], FSTree::empty()).save(&archive_root)?;

    println!("bbup server set up correctly!");

    Ok(())
}
