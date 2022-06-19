use std::{io::BufReader, net::TcpStream};

use bbup_rust::comunications as com;
use bbup_rust::fs;
use bbup_rust::hashtree;
use bbup_rust::utils;

use std::path::PathBuf;

use regex::Regex;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(short, long, value_parser)]
    dir: Option<PathBuf>,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let home_dir = match args.dir {
        Some(val) => val,
        None => dirs::home_dir().expect("could not get home directory"),
    };

    let config: fs::Config = fs::load_yaml(&home_dir.join(".bbup-client").join("config.yaml"))?;

    for link in config.links {
        let link_root = home_dir.join(link);

        println!("{:?}", link_root);

        let link_config: fs::LocalConfig =
            fs::load_yaml(&link_root.join(".bbup").join("config.yaml"))?;

        let mut exclude_list: Vec<Regex> = Vec::new();
        exclude_list.push(Regex::new("\\.bbup/").map_err(utils::to_io_err)?);
        for rule in link_config.exclude_list {
            exclude_list.push(Regex::new(&rule).map_err(utils::to_io_err)?);
        }

        let last_known_update: fs::LastLocalUpdate =
            fs::load_json(&link_root.join(".bbup").join("last_known_update.json"))?;
        let new_tree = hashtree::get_hash_tree(&link_root, &exclude_list)?;

        let local_commit = hashtree::delta(&last_known_update.old_hash_tree, &new_tree);

        println!("{:#?}", local_commit);

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", config.settings.local_port))?;
        let mut input = String::new();
        let mut reader = BufReader::new(stream.try_clone()?);

        let read_value: com::Basic = com::syncrw::read(&mut reader, &mut input)?;
        if read_value.status != 0 {
            return Err(utils::std_err(&read_value.content));
        }

        com::syncrw::write(
            &mut stream,
            com::LastCommit::new(&last_known_update.last_known_commit),
        )?;
    }

    Ok(())
}
