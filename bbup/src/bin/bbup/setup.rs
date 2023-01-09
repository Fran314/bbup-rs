use super::{ClientConfig, ClientSettings};

use abst_fs::AbstPath;

use anyhow::Result;

pub fn setup(home_dir: &AbstPath, opt_local_port: Option<u16>, opt_server_port: Option<u16>, opt_host_name: Option<String>, opt_host_address: Option<String>) -> Result<()> {
    if ClientConfig::exists(home_dir) {
        anyhow::bail!("bbup client is already setup");
    }

    let local_port = match opt_local_port {
        Some(val) => val,
        None => input::get("enter local port (0-65535): ")?.parse::<u16>()?,
    };
    let server_port = match opt_server_port {
        Some(val) => val,
        None => input::get("enter server port (0-65535): ")?.parse::<u16>()?,
    };
    let host_name = match opt_host_name {
        Some(val) => val,
        None => input::get("enter host name: ")?,
    };
    let host_address = match opt_host_address {
        Some(val) => val,
        None => input::get("enter host_address: ")?,
    };


    let settings = ClientSettings {
        local_port,
        server_port,
        host_name,
        host_address,
    };
    ClientConfig::from(settings, Vec::new()).save(home_dir)?;

    println!("bbup client set up correctly!");
    println!();
    println!("run 'bbup init' inside a directory to backup to initialize a backup source");
    println!();

    Ok(())
}
