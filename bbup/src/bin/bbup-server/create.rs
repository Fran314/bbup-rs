use abst_fs::AbstPath;
use anyhow::Result;

use crate::model::{Archive, ArchiveEndpoint};

pub fn create(archive_root: &AbstPath, name: Option<String>) -> Result<()> {
    //
    let name = match name {
        Some(val) => val,
        None => input::get("enter the name of the endpoint: ")?,
    };

    let mut archive_state = Archive::load(archive_root)?;
    let endpoint_state = ArchiveEndpoint::new();
    archive_state.insert(name.clone(), endpoint_state.clone());
    archive_state.save_list(archive_root)?;
    endpoint_state.save(&archive_root.append(&AbstPath::from(name)))?;

    println!("endpoint set up correctly!");
    println!();
    println!("run 'bbup-server run -pv' to start the bbup-server daemon");
    println!();

    Ok(())
}
