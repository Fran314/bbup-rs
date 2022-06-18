use std::fs;
use std::{io::BufReader, net::TcpStream};

use bbup_rust::comunications::syncrw::{read, write};
use bbup_rust::comunications::Basic;
use bbup_rust::hashtree_alternative as hashtree;
use bbup_rust::utils;

use serde::{Deserialize, Serialize};

use std::path::PathBuf;

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

fn read_config(path: &PathBuf) -> std::io::Result<Config> {
    let serialized = fs::read_to_string(&path)?;
    let ht: Config = serde_yaml::from_str(&serialized).map_err(utils::to_io_err)?;
    Ok(ht)
}

fn main() -> std::io::Result<()> {
    // let home_dir = dirs::home_dir().expect("Could not get home directory");
    let home_dir = PathBuf::from("/home/baldo/Documents/playground/bbup");
    let config_path = home_dir.join(".bbup-client").join("config.yaml");

    let config = read_config(&config_path)?;

    for link in config.links {
        let link_root = home_dir.join(link);
        // println!("{:?}", link_root);
        let hashtree_path = link_root.join(".bbup").join("hashtree.json");
        // let old_tree = hashtree::load_tree(&hashtree_path)?;
        let new_tree = hashtree::hash_tree(
            &std::path::Path::new("/home/baldo/Documents/playground").to_path_buf(),
            &std::path::Path::new("").to_path_buf(),
            &vec![Regex::new(".bbup/").map_err(utils::to_io_err)?],
            // &Vec::new(),
        )?;
        // println!("{:#?}", &new_tree);
        hashtree::save_tree(&hashtree_path, &new_tree)?;
        // println!("{:#?}", hashtree::delta(&old_tree, &new_tree));
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
