use crate::smodel::ArchiveState;
use crate::ServerConfig;

use bbup_rust::fs::{list_dir_content, AbstPath, ObjectType};
use bbup_rust::fstree::FSTree;
use bbup_rust::input;
use bbup_rust::model::Commit;

use anyhow::Result;

pub fn setup(home_dir: AbstPath) -> Result<()> {
    if ServerConfig::exists(&home_dir) {
        anyhow::bail!("bbup server is already setup");
    }

    let server_port = input::get("enter server port (0-65535): ")?.parse::<u16>()?;
    let archive_root = AbstPath::from(input::get("enter archive root (relative to ~): ")?);
    let absolute_archive_root = home_dir.append(&archive_root);
    match absolute_archive_root.object_type() {
        Some(ObjectType::Dir) => {}
        Some(_) => anyhow::bail!("specified archive root is not a directory!"),
        None => anyhow::bail!("specified archive root does not exist!"),
    }
    // TODO maybe make so that an archive can be setup from a non-empty state
    if !list_dir_content(&absolute_archive_root)?.is_empty() {
        anyhow::bail!("specified archive root is not empty!");
    }

    ServerConfig::from(server_port, archive_root).save(&home_dir)?;
    ArchiveState::from(vec![Commit::base_commit()], FSTree::empty())
        .save(&absolute_archive_root)?;

    println!("bbup server set up correctly!");
    println!();
    println!("run 'bbup-server run -pv' to start the bbup-server daemon");
    println!();

    Ok(())
}
