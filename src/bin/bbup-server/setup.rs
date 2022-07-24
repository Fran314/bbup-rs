use crate::{CommitList, ServerConfig};

use bbup_rust::fstree::FSTree;
use bbup_rust::model::Commit;
use bbup_rust::{fs, input};

use std::path::PathBuf;

use anyhow::Result;

pub fn setup(home_dir: PathBuf) -> Result<()> {
    if home_dir.join(".config").join("bbup-server").exists()
        && home_dir
            .join(".config")
            .join("bbup-server")
            .join("config.yaml")
            .exists()
    {
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

    fs::create_dir(&home_dir.join(".config").join("bbup-server"))?;

    fs::save(
        &home_dir
            .join(".config")
            .join("bbup-server")
            .join("config.yaml"),
        &ServerConfig {
            server_port,
            archive_root: archive_root.clone(),
        },
    )?;

    let archive_path = home_dir.join(archive_root);
    // TODO, this is probably not necessary but check
    // std::fs::create_dir_all(archive_path.join(".bbup").join("temp"))?;

    fs::create_dir(&archive_path.join(".bbup"))?;

    let commit_list: CommitList = vec![Commit::base_commit()];
    fs::save(
        &archive_path.join(".bbup").join("commit-list.json"),
        &commit_list,
    )?;

    fs::save(
        &archive_path.join(".bbup").join("archive-tree.json"),
        &FSTree::empty(),
    )?;

    println!("bbup server set up correctly!");

    Ok(())
}
