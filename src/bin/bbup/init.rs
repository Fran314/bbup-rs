use crate::{cmodel::ProcessState, LinkConfig, LinkType};

use bbup_rust::{fstree, input, model::Commit};

use std::path::PathBuf;

use anyhow::Result;

pub fn init(cwd: PathBuf) -> Result<()> {
    if LinkConfig::exists(&cwd) {
        anyhow::bail!(
            "Current directory [{:?}] is already initialized as a backup source",
            cwd
        )
    }

    let endpoint: Vec<String> = loop {
        let path = input::get("set endpoint (relative to archive root): ")?;
        // TODO: do all sorts of checks:
        //	- make sure it's a relative path
        //	- check which separator is used
        //	- ask for confirmation
        break path.split("/").map(|s| s.to_string()).collect();
    };

    let mut exclude_list: Vec<String> = Vec::new();
    let add_exclude_list = input::get("add exclude list [y/N]?: ")?;
    if add_exclude_list.to_ascii_lowercase().eq("y") {
        println!("add regex rules in string form. To stop, enter empty string");
        loop {
            let rule = input::get("rule: ")?;
            if rule.eq("") {
                break;
            }
            exclude_list.push(rule);
        }
    }
    LinkConfig::from(LinkType::Bijection, endpoint, exclude_list).save(&cwd)?;
    ProcessState::from(Commit::base_commit().commit_id, fstree::FSTree::empty()).save(&cwd)?;

    println!("backup source initialized correctly!");
    println!("");
    println!("run 'bbup sync -pv' to download the curent state of the endpoint");
    println!("");

    Ok(())
}
