use std::path::PathBuf;
use tokio::net::TcpStream;

use bbup_rust::comunications::BbupCom;
use bbup_rust::{fs, hashtree, structs, utils};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;

#[derive(Subcommand, Debug)]
enum SubCommand {
    /// Pull updates from server and push local updates
    Sync {
        /// Increase verbosity
        #[clap(short, long, value_parser)]
        verbose: bool,

        /// Show progress in file upload and download
        #[clap(short, long, value_parser)]
        progress: bool,

        /// Show tcp transcript minimized
        #[clap(short = 'A', long, value_parser)]
        all: bool,
    },
    /// Initialize link
    Init,
}

#[derive(Parser, Debug)]
#[clap(name = "bbup", version)]
struct Args {
    /// Custom home directory for testing
    #[clap(long, value_parser)]
    home_dir: Option<PathBuf>,

    #[clap(subcommand)]
    cmd: SubCommand,
}

struct CommitState {
    link_root: PathBuf,
    endpoint: PathBuf,
    exclude_list: Vec<Regex>,
    last_known_commit: Option<String>,
    old_tree: Option<hashtree::HashTreeNode>,
    new_tree: Option<hashtree::HashTreeNode>,
    local_delta: Option<structs::Delta>,
    update: Option<structs::ClientUpdate>,
}
impl CommitState {
    fn init(link_root: PathBuf, endpoint: PathBuf, exclude_list: Vec<Regex>) -> CommitState {
        CommitState {
            link_root,
            endpoint,
            exclude_list,
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
    let local_delta = hashtree::delta(&old_tree, &new_tree);
    state.old_tree = Some(old_tree);
    state.new_tree = Some(new_tree);

    state.local_delta = Some(local_delta);
    Ok(())
}
async fn pull_update_delta<T, R>(state: &mut CommitState, com: &mut BbupCom<T, R>) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    // [PULL] Send last known commit to pull updates in case of any
    com.send_struct(
		structs::UpdateRequest {
			endpoint: state.endpoint.clone(),
			lkc: state.last_known_commit.clone().context("last-known-commit is necessary for pull-update-delta call. Expected Some(_), found None")?
		}
    ).await
    .context("could not send last known commit")?;

    // [PULL] Get delta from last_known_commit to server's most recent commit
    let mut update: structs::ClientUpdate = com
        .get_struct()
        .await
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
async fn check_for_conflicts(state: &mut CommitState) -> Result<()> {
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

async fn download_update<T, R>(state: &mut CommitState, com: &mut BbupCom<T, R>) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    let update = state
        .update
        .clone()
        .context("update is necessary for download-update call. Expected Some(_), found None")?;

    for change in &update.delta {
        if change.action != structs::Action::Removed
            && change.object_type != structs::ObjectType::Dir
        {
            com.send_struct(Some(change.path.clone())).await?;
            com.get_file_to(
                &state
                    .link_root
                    .join(".bbup")
                    .join("temp")
                    .join(change.path.clone()),
            )
            .await?;
        }
    }

    com.send_struct(None::<PathBuf>).await?;
    Ok(())
}
async fn apply_update(state: &mut CommitState) -> Result<()> {
    let update = state
        .update
        .clone()
        .context("update is necessary for download-update call. Expected Some(_), found None")?;
    for change in &update.delta {
        let path = state.link_root.join(&change.path);
        let from_temp_path = state
            .link_root
            .join(".bbup")
            .join("temp")
            .join(&change.path);
        match (change.action, change.object_type) {
            (structs::Action::Removed, structs::ObjectType::Dir) => std::fs::remove_dir(&path)
                .context(format!(
                    "could not remove directory to apply update\npath: {:?}",
                    path
                ))?,
            (structs::Action::Removed, _) => std::fs::remove_file(&path).context(format!(
                "could not remove file to apply update\npath: {:?}",
                path
            ))?,
            (structs::Action::Added, structs::ObjectType::Dir) => std::fs::create_dir(&path)
                .context(format!(
                    "could not create directory to apply update\npath: {:?}",
                    path
                ))?,
            (structs::Action::Edited, structs::ObjectType::Dir) => {
                unreachable!("Dir cannot be edited: broken update delta")
            }
            (structs::Action::Added, _) | (structs::Action::Edited, _) => {
                std::fs::copy(&from_temp_path, &path).context(format!(
                    "could not copy file from temp to apply update\npath: {:?}",
                    path
                ))?;
            }
        }
    }
    match (&mut state.old_tree, &state.new_tree) {
        (Some(old_tree), Some(new_tree)) => {
            old_tree.apply_delta(&update.delta)?;
            fs::save(
                &state.link_root.join(".bbup").join("old-hash-tree.json"),
                &old_tree,
            )?;
            fs::save(
                &state.link_root.join(".bbup").join("last-known-commit.json"),
                &update.commit_id,
            )?;

            let local_delta = hashtree::delta(&old_tree, &new_tree);
            state.local_delta = Some(local_delta);
        }
        _ => todo!(),
    };

    Ok(())
}
async fn upload_changes<T, R>(state: &mut CommitState, com: &mut BbupCom<T, R>) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    // Await green light to procede
    com.check_ok().await?;

    let local_delta = state.local_delta.clone().context(
        "local delta is necessary for upload-changes call. Expected Some(_), found None",
    )?;

    com.send_struct(local_delta).await?;

    loop {
        let path: Option<PathBuf> = com.get_struct().await?;
        let path = match path {
            Some(val) => val,
            None => break,
        };
        com.send_file_from(&state.link_root.join(path)).await?;
    }

    let new_commit_id: String = com.get_struct().await?;

    match &state.new_tree {
        Some(new_tree) => {
            fs::save(
                &state.link_root.join(".bbup").join("old-hash-tree.json"),
                &new_tree,
            )?;
            fs::save(
                &state.link_root.join(".bbup").join("last-known-commit.json"),
                &new_commit_id,
            )?;
        }
        None => todo!(),
    }

    Ok(())
}

async fn process_link(link: &String, config: &fs::ClientConfig, home_dir: &PathBuf) -> Result<()> {
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
    let socket = TcpStream::connect(format!("127.0.0.1:{}", config.settings.local_port))
        .await
        .context("could not connect to server")?;
    // let (mut rx, mut tx) = socket.into_split();

    let mut com = BbupCom::from_split(socket.into_split(), true);

    // com.send_file_from(&PathBuf::from("foo.txt")).await?;

    // tx.send_file_from(&PathBuf::from("foo.txt")).await?;
    // tx.send_struct(String::from("AAAAA")).await?;

    // Await green light to procede
    com.check_ok()
        .await
        .context("could not get green light from server to procede with conversation")?;

    com.send_struct(&link_config.endpoint).await?;

    let mut state = CommitState::init(link_root, link_config.endpoint, exclude_list);

    get_local_delta(&mut state)?;
    pull_update_delta(&mut state, &mut com).await?;
    check_for_conflicts(&mut state).await?;
    download_update(&mut state, &mut com).await?;
    apply_update(&mut state).await?;
    upload_changes(&mut state, &mut com).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let home_dir = match args.home_dir {
        Some(val) => Some(val),
        None => dirs::home_dir(),
    }
    .context("could not resolve home_dir path")?;

    match args.cmd {
        SubCommand::Sync {
            verbose: _verbose,
            progress: _progress,
            all: _all,
        } => {
            let config: fs::ClientConfig = fs::load(
                &home_dir
                    .join(".config")
                    .join("bbup-client")
                    .join("config.yaml"),
            )?;

            for link in &config.links {
                match process_link(&link, &config, &home_dir).await {
                    Ok(_) => {
                        println!("{} correctly processed", link);
                    }
                    Err(err) => {
                        println!("Failed to process link {}\n{:?}", link, err);
                    }
                };
            }
        }
        SubCommand::Init => {
            println!(
                "eeeee \x1b[1m\x1b[33mError\x1b[34mError\x1b[36mError\x1b[0mProvaprova sa sa sa"
            )
        }
    }

    Ok(())
}