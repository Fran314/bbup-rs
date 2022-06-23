use std::{
    io::{BufReader, Write},
    net::TcpStream,
    path::PathBuf,
};

use bbup_rust::{comunications as com, fs, hashtree, structs, utils};

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(short, long, value_parser)]
    dir: Option<PathBuf>,
}

struct CommitState {
    link_root: PathBuf,
    config: fs::ClientConfig,
    endpoint: PathBuf,
    exclude_list: Vec<Regex>,
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    last_known_commit: Option<String>,
    old_tree: Option<hashtree::HashTreeNode>,
    new_tree: Option<hashtree::HashTreeNode>,
    local_delta: Option<structs::Delta>,
    update: Option<structs::ClientUpdate>,
}
impl CommitState {
    fn init(
        link_root: PathBuf,
        config: fs::ClientConfig,
        endpoint: PathBuf,
        exclude_list: Vec<Regex>,
        stream: TcpStream,
        reader: BufReader<TcpStream>,
    ) -> CommitState {
        CommitState {
            link_root,
            config,
            endpoint,
            exclude_list,
            stream,
            reader,
            last_known_commit: None,
            old_tree: None,
            new_tree: None,
            local_delta: None,
            update: None,
        }
    }
}

fn get_local_delta(state: &mut CommitState) -> Result<()> {
    state.last_known_commit = Some(fs::load(
        &state.link_root.join(".bbup").join("last-known-commit.json"),
    )?);
    let old_tree = fs::load(&state.link_root.join(".bbup").join("old-hash-tree.json"))?;
    let new_tree = hashtree::get_hash_tree(&state.link_root, &state.exclude_list)?;

    state.local_delta = Some(hashtree::delta(&old_tree, &new_tree));
    state.old_tree = Some(old_tree);
    state.new_tree = Some(new_tree);
    Ok(())
}
fn pull_update_delta(state: &mut CommitState) -> Result<()> {
    let mut input = String::new();

    // [PULL] Send last known commit to pull updates in case of any
    com::syncrw::write(
        &mut state.stream,
        0,
		structs::UpdateRequest {
			endpoint: state.endpoint.clone(),
			lkc: state.last_known_commit.clone().context("last-known-commit is necessary for pull-update-delta call. Expected Some(_), found None")?
		},
        "last known commit",
    )
    .context("could not send last known commit")?;

    // [PULL] Get delta from last_known_commit to server's most recent commit
    let mut update: structs::ClientUpdate = com::syncrw::read(&mut state.reader, &mut input)
        .context("could not get update-delta from server")?;

    // [PULL] Filter out updates that match the exclude_list
    update.delta.retain(
        |item| !match utils::to_exclude(&item.path, &state.exclude_list) {
            Ok(val) => val,
            Err(_) => false,
        },
    );

    state.update = Some(update);
    Ok(())
}
fn check_for_conflicts(state: &mut CommitState) -> Result<()> {
    let local_delta = &state.local_delta.clone().context(
        "local-delta is necessary for check-for-conflicts call. Expected Some(_), found None",
    )?;
    let update_delta = &state
        .update
        .clone()
        .context("update is necessary for check-for-conflicts call. Expected Some(_), found None")?
        .delta;
    let conflicts = local_delta.into_iter().any(|local_change| {
        update_delta.into_iter().any(|update_change| {
            if local_change.path.eq(&update_change.path) {
                local_change.hash.ne(&update_change.hash)
            } else {
                local_change.path.starts_with(&update_change.path)
                    || update_change.path.starts_with(&local_change.path)
            }
        })
    });
    if conflicts {
        return Err(anyhow::Error::new(utils::std_err(
            "found conflicts between pulled update and local changes. Resolve manually",
        )));
    }
    Ok(())
}

fn download_update(state: &mut CommitState) -> Result<()> {
    let update = state
        .update
        .clone()
        .context("update is necessary for download-update call. Expected Some(_), found None")?;
    let mut child = std::process::Command::new("rsync")
        .arg("-az")
        .arg("--files-from=-")
        .arg(format!(
            "{}@{}:{}",
            state.config.settings.host_name,
            state.config.settings.host_address,
            update.root.to_str().context(format!(
                "invalid root for update download, possibly invalid utf8\nroot path: {:?}",
                update.root
            ))?
        ))
        .arg(state.link_root.join(".bbup").join("temp"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn rsync process")?;

    let mut stdin = String::new();
    for change in &update.delta {
        if change.action == structs::Action::Added || change.action == structs::Action::Edited {
            stdin += change.path.to_str().context(format!(
                "cannot convert path of change to str, possibly invalid utf8\nchange: {:#?}",
                change.path
            ))?;
            stdin += "\n";
        }
    }
    let child_stdin = child.stdin.as_mut().unwrap();
    child_stdin.write_all(stdin.as_bytes())?;
    // Close stdin to finish and avoid indefinite blocking
    drop(child_stdin);

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow::Error::new(utils::std_err(
            format!(
                "rsync to download update didn't resolve correctly. Exit status is {:?}",
                status
            )
            .as_str(),
        )));
    }
    Ok(())
}
fn apply_update(_state: &mut CommitState) -> Result<()> {
    todo!();
}
fn upload_changes(_state: &mut CommitState) -> Result<()> {
    todo!();
}

fn process_link(link: &String, config: &fs::ClientConfig, home_dir: &PathBuf) -> Result<()> {
    let link_root = home_dir.join(link);

    // Parse Link configs
    let link_config: fs::LinkConfig = fs::load(&link_root.join(".bbup").join("config.yaml"))?;
    let mut exclude_list: Vec<Regex> = Vec::new();
    exclude_list
        .push(Regex::new("\\.bbup/").context("could not generate regex from .bbup pattern")?);
    for rule in &link_config.exclude_list {
        exclude_list.push(
            Regex::new(&rule).context(
                "could not generate regex from pattern from exclude_list in link config",
            )?,
        );
    }

    // Start connection
    let stream = TcpStream::connect(format!("127.0.0.1:{}", config.settings.local_port))
        .context("could not connect to server")?;
    let mut input = String::new();
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .context("error on converting stream to buffer reader")?,
    );

    // Await green light to procede
    let _: com::Empty = com::syncrw::read(&mut reader, &mut input)
        .context("could not get green light from server to procede with conversation")?;

    let mut state = CommitState::init(
        link_root,
        config.clone(),
        link_config.endpoint,
        exclude_list,
        stream,
        reader,
    );

    get_local_delta(&mut state)?;
    // fs::save(&PathBuf::from("test.json"), &state.local_delta)?;
    pull_update_delta(&mut state)?;
    // println!("{:#?}", state.update);
    check_for_conflicts(&mut state)?;
    download_update(&mut state)?;
    apply_update(&mut state)?;
    upload_changes(&mut state)?;

    Ok(())
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

    for link in &config.links {
        match process_link(&link, &config, &home_dir) {
            Ok(_) => {
                println!("{} correctly processed", link);
            }
            Err(err) => {
                println!("Failed to process link {}\n{:?}", link, err);
            }
        };
    }

    Ok(())
}
