use crate::model::SetupOps;

use super::{ClientConfig, ClientSettings};

use abst_fs::AbstPath;

use anyhow::Result;

pub fn setup(conf_dir: &AbstPath, options: SetupOps) -> Result<()> {
    if ClientConfig::exists(conf_dir) {
        anyhow::bail!("bbup client is already setup");
    }

    let local_port = match options.local_port {
        Some(val) => val,
        None => input::get("enter local port (0-65535): ")?.parse::<u16>()?,
    };
    let server_port = match options.server_port {
        Some(val) => val,
        None => input::get("enter server port (0-65535): ")?.parse::<u16>()?,
    };
    let host_name = match options.host_name {
        Some(val) => val,
        None => input::get("enter host name: ")?,
    };
    let host_address = match options.host_address {
        Some(val) => val,
        None => input::get("enter host_address: ")?,
    };

    let settings = ClientSettings {
        local_port,
        server_port,
        host_name,
        host_address,
    };
    ClientConfig::from(settings, Vec::new()).save(conf_dir)?;

    println!("bbup client set up correctly!");
    println!();
    println!("run 'bbup init' inside a directory to backup to initialize a backup source");
    println!();

    Ok(())
}
