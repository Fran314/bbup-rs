use std::path::PathBuf;

use crate::{ClientConfig, ClientSettings};

use bbup_rust::input;

use anyhow::Result;

pub fn setup(home_dir: PathBuf) -> Result<()> {
    if ClientConfig::exists(&home_dir) {
        anyhow::bail!("bbup client is already setup");
    }

    let local_port = input::get("enter local port (0-65535): ")?.parse::<u16>()?;
    let server_port = input::get("enter server port (0-65535): ")?.parse::<u16>()?;
    let host_name = input::get("enter host name: ")?;
    let host_address = input::get("enter host address: ")?;

    let settings = ClientSettings {
        local_port,
        server_port,
        host_name,
        host_address,
    };
    ClientConfig::from(settings, Vec::new()).save(&home_dir)?;

    println!("bbup client set up correctly!");

    Ok(())
}
