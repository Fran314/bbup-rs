use std::{io::BufReader, net::TcpStream};

use bbup_rust::comunications as com;
use bbup_rust::fs;
use bbup_rust::hashtree;
use bbup_rust::utils;

use std::path::PathBuf;

use regex::Regex;

fn main() -> std::io::Result<()> {
    // FOR DEBUG AND TEST ONLY
    let home_dir = PathBuf::from("/home/baldo/Documents/playground/bbup");
    // let home_dir = dirs::home_dir().expect("could not get home directory");

    let config: fs::Config = fs::load_yaml(&home_dir.join(".bbup-client").join("config.yaml"))?;

    for link in config.links {
        let link_root = home_dir.join(link);
        let local_config_path = link_root.join(".bbup").join("config.yaml");
        let link_config: fs::LocalConfig = fs::load_yaml(&local_config_path)?;

        let mut exclude_list: Vec<Regex> = Vec::new();
        exclude_list.push(Regex::new("\\.bbup/").map_err(utils::to_io_err)?);
        for rule in link_config.exclude_list {
            exclude_list.push(Regex::new(&rule).map_err(utils::to_io_err)?);
        }

        let hashtree_path = link_root.join(".bbup").join("hashtree.json");
        let old_tree: hashtree::HashTreeNode = fs::load_json(&hashtree_path)?;
        let new_tree = hashtree::get_hash_tree(&link_root, &exclude_list)?;
        let local_commit = hashtree::delta(&old_tree, &new_tree);

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", config.settings.local_port))?;
        let mut input = String::new();
        let mut reader = BufReader::new(stream.try_clone()?);

        let read_value: com::Basic = com::syncrw::read(&mut reader, &mut input)?;
        println!("Recieved from server: {}", read_value.content);

        com::syncrw::write(&mut stream, com::Basic::new(0, "Hello server"))?;

        let read_value: com::Basic = com::syncrw::read(&mut reader, &mut input)?;
        println!("Recieved from server: {}", read_value.content);
    }

    Ok(())
}
