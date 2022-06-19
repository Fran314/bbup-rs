use std::{io::BufReader, net::TcpStream};

use bbup_rust::{comunications as com, fs, hashtree, utils};

use std::path::PathBuf;

use regex::Regex;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(short, long, value_parser)]
    dir: Option<PathBuf>,
}

fn check_for_conflicts(local_delta: &fs::Delta, update_delta: &fs::Delta) -> bool {
    local_delta.into_iter().any(|local_change| {
        update_delta.into_iter().any(|update_change| {
            if local_change.path.eq(&update_change.path) {
                local_change.hash.ne(&update_change.hash)
            } else {
                local_change.path.starts_with(&update_change.path)
                    || update_change.path.starts_with(&local_change.path)
            }
        })
    })
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let home_dir = match args.dir {
        Some(val) => val,
        None => dirs::home_dir().expect("could not get home directory"),
    };

    let config: fs::ClientConfig =
        fs::load_yaml(&home_dir.join(".bbup-client").join("config.yaml"))?;

    for link in config.links {
        let link_root = home_dir.join(link);

        let link_config: fs::LinkConfig =
            fs::load_yaml(&link_root.join(".bbup").join("config.yaml"))?;

        let mut exclude_list: Vec<Regex> = Vec::new();
        exclude_list.push(Regex::new("\\.bbup/").map_err(utils::to_io_err)?);
        for rule in link_config.exclude_list {
            exclude_list.push(Regex::new(&rule).map_err(utils::to_io_err)?);
        }

        let last_known_commit: String =
            fs::load_json(&link_root.join(".bbup").join("last-known-commit.json"))?;
        let old_hash_tree: hashtree::HashTreeNode =
            fs::load_json(&link_root.join(".bbup").join("old-hash-tree.json"))?;
        let new_tree = hashtree::get_hash_tree(&link_root, &exclude_list)?;

        let local_delta: fs::Delta = hashtree::delta(&old_hash_tree, &new_tree);

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", config.settings.local_port))?;
        let mut input = String::new();
        let mut reader = BufReader::new(stream.try_clone()?);

        // Await response from client
        //	0 => not busy, comunication can procede
        //	!0 => busy, retry later
        let green_light: com::Basic = com::syncrw::read(&mut reader, &mut input)?;
        if green_light.status != 0 {
            return Err(utils::std_err(&green_light.content));
        }

        // [PULL] Send last known commit to pull updates in case of any
        com::syncrw::write(&mut stream, com::LastCommit::new(&last_known_commit))?;

        // [PULL] Get delta from last_known_commit to server's most recent commit
        let update_delta: fs::Commit = com::syncrw::read(&mut reader, &mut input)?;
        println!(
            "{:#?}",
            check_for_conflicts(&local_delta, &update_delta.delta)
        );

        // ...

        // ...

        // fs::save_json(
        //     &link_root.join(".bbup").join("old_hash_tree.json"),
        //     &new_tree,
        // )?;
        // fs::save_json(
        //     &link_root.join(".bbup").join("last_known_commit.json"),
        //     &"AAAAAAAA".to_string(),
        // )?;
    }

    Ok(())
}
