use crate::{CommitList, ServerConfig};

use bbup_rust::hashtree::Tree;
use bbup_rust::structs::Commit;
use bbup_rust::{fs, io};

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

    std::fs::create_dir_all(home_dir.join(".config").join("bbup-server"))?;
    let server_port = io::get_input("enter server port (0-65535): ")?.parse::<u16>()?;
    let archive_root = PathBuf::from(io::get_input("enter archive root (relative to ~): ")?);
    if !home_dir.join(&archive_root).exists() {
        anyhow::bail!("specified archive root does not exist!");
    }
    if !home_dir.join(&archive_root).is_dir() {
        anyhow::bail!("specified archive root is not a directory!");
    }
    if !home_dir.join(&archive_root).read_dir()?.next().is_none() {
        anyhow::bail!("specified archive root is not empty!");
    }

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
    std::fs::create_dir_all(archive_path.join(".bbup").join("temp"))?;

    let commit_list: CommitList = vec![Commit::base_commit()];
    fs::save(
        &archive_path.join(".bbup").join("commit-list.json"),
        &commit_list,
    )?;

    fs::save(
        &archive_path.join(".bbup").join("archive-tree.json"),
        &Tree::empty_node(),
    )?;

    println!("bbup server set up correctly!");

    Ok(())
}
