use crate::{LinkConfig, LinkType};

use bbup_rust::{fs, hashtree, io, model::ExcludeList, path::AbstractPath};

use std::path::PathBuf;

use anyhow::Result;

pub fn init(cwd: PathBuf) -> Result<()> {
    if cwd.join(".bbup").exists() && cwd.join(".bbup").join("config.yaml").exists() {
        anyhow::bail!(
            "Current directory [{:?}] is already initialized as a backup source",
            cwd
        )
    }
    if !cwd.join(".bbup").exists() {
        std::fs::create_dir_all(cwd.join(".bbup"))?;
    }
    // TODO this should somehow convert to AbstractPath
    let endpoint = PathBuf::from(io::get_input("set endpoint (relative to archive root): ")?);
    let add_exclude_list = io::get_input("add exclude list [y/N]?: ")?;
    let mut exclude_list: Vec<String> = Vec::new();
    if add_exclude_list.to_ascii_lowercase().eq("y")
        || add_exclude_list.to_ascii_lowercase().eq("yes")
    {
        println!("add regex rules in string form. To stop, enter empty string");
        loop {
            let rule = io::get_input("rule: ")?;
            if rule.eq("") {
                break;
            }
            exclude_list.push(rule);
        }
    }
    let local_config = LinkConfig {
        link_type: LinkType::Bijection,
        // TODO see above, where endpoint should be read as AbstractPath. This is a quickfix
        endpoint: AbstractPath::from(endpoint)?,
        exclude_list: exclude_list.clone(),
    };

    fs::save(&cwd.join(".bbup").join("config.yaml"), &local_config)?;
    let tree = hashtree::generate_hash_tree(&cwd, &ExcludeList::from(&exclude_list)?)?;
    fs::save(&cwd.join(".bbup").join("old-hash-tree.json"), &tree)?;
    fs::save(
        &cwd.join(".bbup").join("last-known-commit.json"),
        &String::from("0").repeat(64),
    )?;

    println!("backup source initialized correctly!");

    Ok(())
}
