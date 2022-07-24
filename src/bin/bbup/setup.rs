use std::path::PathBuf;

use crate::{ClientConfig, ClientSettings};

use bbup_rust::{fs, input};

use anyhow::Result;

pub fn setup(home_dir: PathBuf) -> Result<()> {
    if home_dir.join(".config").join("bbup-client").exists()
        && home_dir
            .join(".config")
            .join("bbup-client")
            .join("config.yaml")
            .exists()
    {
        anyhow::bail!("bbup client is already setup");
    }

    let local_port = input::get("enter local port (0-65535): ")?.parse::<u16>()?;
    let server_port = input::get("enter server port (0-65535): ")?.parse::<u16>()?;
    let host_name = input::get("enter host name: ")?;
    let host_address = input::get("enter host address: ")?;

    fs::create_dir(&home_dir.join(".config").join("bbup-client"))?;

    let settings = ClientSettings {
        local_port,
        server_port,
        host_name,
        host_address,
    };
    fs::save(
        &home_dir
            .join(".config")
            .join("bbup-client")
            .join("config.yaml"),
        &ClientConfig {
            settings,
            links: Vec::new(),
        },
    )?;

    println!("bbup client set up correctly!");

    Ok(())
}
