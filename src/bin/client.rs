use std::{io::BufReader, net::TcpStream, path::PathBuf};

use bbup_rust::{comunications as com, fs, hashtree, structs, utils};

use regex::Regex;

use clap::Parser;

use anyhow::{Context, Result};

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(short, long, value_parser)]
    dir: Option<PathBuf>,
}

fn check_for_conflicts(local_delta: &structs::Delta, update_delta: &structs::Delta) -> bool {
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

fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let home_dir = match args.dir {
        Some(val) => Some(val),
        None => dirs::home_dir(),
    }
    .context("could not resolve home_dir path")?;

    let config: fs::ClientConfig = fs::load(
        &home_dir
            .join(".config")
            .join("bbup-client")
            .join("config.yaml"),
    )?;

    for link in config.links {
        let link_root = home_dir.join(link);

        let link_config: fs::LinkConfig = fs::load(&link_root.join(".bbup").join("config.yaml"))?;

        let mut exclude_list: Vec<Regex> = Vec::new();
        exclude_list.push(Regex::new("\\.bbup/").map_err(utils::to_io_err)?);
        for rule in link_config.exclude_list {
            exclude_list.push(Regex::new(&rule).map_err(utils::to_io_err)?);
        }

        let last_known_commit: String =
            fs::load(&link_root.join(".bbup").join("last-known-commit.json"))?;
        let old_hash_tree: hashtree::HashTreeNode =
            fs::load(&link_root.join(".bbup").join("old-hash-tree.json"))?;
        let new_tree = hashtree::get_hash_tree(&link_root, &exclude_list)?;

        let local_delta: structs::Delta = hashtree::delta(&old_hash_tree, &new_tree);

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", config.settings.local_port))?;
        let mut input = String::new();
        let mut reader = BufReader::new(stream.try_clone()?);

        // Await green light to procede
        let _: com::Empty = com::syncrw::read(&mut reader, &mut input)
            .context("could not get green light from server to procede with conversation")?;

        // [PULL] Send last known commit to pull updates in case of any
        com::syncrw::write(
            &mut stream,
            0,
            last_known_commit.clone(),
            "last known commit",
        )
        .context("could not send last known commit")?;

        // [PULL] Get delta from last_known_commit to server's most recent commit
        let update_delta: structs::Commit = com::syncrw::read(&mut reader, &mut input)
            .context("could not get update-delta from server")?;

        println!(
            "{:#?}",
            check_for_conflicts(&local_delta, &update_delta.delta)
        );

        // Rest of protocol
    }

    Ok(())
}
