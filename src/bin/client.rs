use std::fs;
use std::{io::BufReader, net::TcpStream};

use bbup_rust::comunications::syncrw::{read, write};
use bbup_rust::comunications::Basic;
use bbup_rust::hashtree;
use bbup_rust::path;
use bbup_rust::utils;

use serde::{Deserialize, Serialize};

use regex::Regex;

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    settings: Settings,
    links: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Settings {
    local_port: u16,
    server_port: u16,
    host_name: String,
    host_address: String,
}

fn read_config(path: &str) -> std::io::Result<Config> {
    let serialized = fs::read_to_string(&path)?;
    let ht: Config = serde_yaml::from_str(&serialized).map_err(utils::to_io_err)?;
    Ok(ht)
}

fn main() -> std::io::Result<()> {
    // let home_dir = dirs::home_dir().expect("Could not get home directory");
    // let home_dir = home_dir.to_str().expect("Could not get home directory");
    let home_dir = "/home/baldo/Documents/playground/bbup";
    let config_path = path![home_dir, ".bbup-client", "config.yaml"];

    let config = read_config(&config_path)?;

    for link in config.links {
        let link_root = path![home_dir, link.as_str()];
        println!("{}", link_root);
        let old_tree =
            hashtree::load_tree(path![link_root.as_str(), ".bbup", "hashtree.json"].as_str())?;
        let new_tree = hashtree::hash_tree(
            &link_root,
            &vec![Regex::new(".bbup").map_err(utils::to_io_err)?],
        )?;
        hashtree::save_tree(
            path![link_root.as_str(), ".bbup", "hashtree.json"].as_str(),
            &new_tree,
        )?;
        println!("{:#?}", hashtree::delta(&old_tree, &new_tree));
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", config.settings.local_port))?;
        let mut input = String::new();
        let mut reader = BufReader::new(stream.try_clone()?);

        let read_value: Basic = read(&mut reader, &mut input)?;
        println!("Recieved from server: {}", read_value.content);

        write(&mut stream, Basic::new("Hello server"))?;

        let read_value: Basic = read(&mut reader, &mut input)?;
        println!("Recieved from server: {}", read_value.content);
    }

    Ok(())
}
