use crate::{LinkConfig, LinkType};

use bbup_rust::{fs, fstree, input, model::ExcludeList};

use std::path::PathBuf;

use anyhow::Result;

pub fn init(cwd: PathBuf) -> Result<()> {
    if cwd.join(".bbup").exists() && cwd.join(".bbup").join("config.yaml").exists() {
        anyhow::bail!(
            "Current directory [{:?}] is already initialized as a backup source",
            cwd
        )
    }

    let endpoint: Vec<String> = loop {
        let path = input::get("set endpoint (relative to archive root): ")?;
        // TOTO: do all sorts of checks:
        //	- make sure it's a relative path
        //	- check which separator is used
        //	- ask for confirmation
        break path.split("/").map(|s| s.to_string()).collect();
    };

    let mut exclude_list: Vec<String> = Vec::new();
    let add_exclude_list = input::get("add exclude list [y/N]?: ")?;
    if add_exclude_list.to_ascii_lowercase().eq("y")
        || add_exclude_list.to_ascii_lowercase().eq("yes")
    {
        println!("add regex rules in string form. To stop, enter empty string");
        loop {
            let rule = input::get("rule: ")?;
            if rule.eq("") {
                break;
            }
            exclude_list.push(rule);
        }
    }
    let local_config = LinkConfig {
        link_type: LinkType::Bijection,
        endpoint,
        exclude_list: exclude_list.clone(),
    };

    fs::save(&cwd.join(".bbup").join("config.yaml"), &local_config)?;
    let tree = fstree::generate_fstree(&cwd, &ExcludeList::from(&exclude_list)?)?;
    fs::save(&cwd.join(".bbup").join("old-hash-tree.json"), &tree)?;
    fs::save(
        &cwd.join(".bbup").join("last-known-commit.json"),
        &String::from("0").repeat(64),
    )?;

    println!("backup source initialized correctly!");

    Ok(())
}
