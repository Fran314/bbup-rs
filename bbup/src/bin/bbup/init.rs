use super::{LinkConfig, LinkType, ProcessState};

use abst_fs::AbstPath;

use anyhow::Result;

pub fn init(cwd: &AbstPath) -> Result<()> {
    if LinkConfig::exists(cwd) {
        anyhow::bail!("Current directory [{cwd}] is already initialized as a backup source")
    }

    // TODO: do all sorts of checks:
    //	- make sure it's a relative path
    //	- check which separator is used
    //	- ask for confirmation
    let endpoint = AbstPath::from(input::get("set endpoint (relative to archive root): ")?);

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
    LinkConfig::from(LinkType::Bijection, endpoint, exclude_list).save(cwd)?;
    ProcessState::init_state().save(cwd)?;

    println!("backup source initialized correctly!");
    println!();
    println!("run 'bbup sync -pv' to download the curent state of the endpoint");
    println!();

    Ok(())
}
