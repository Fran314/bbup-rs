use std::path::PathBuf;
use tokio::net::TcpStream;

use bbup_rust::com::BbupCom;
use bbup_rust::structs::PrettyPrint;
use bbup_rust::{com, fs, hashtree, io, ssh_tunnel, structs, utils};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;

#[derive(Subcommand, Debug, PartialEq)]
enum SubCommand {
    /// Pull updates from server and push local updates
    Sync {
        /// Increase verbosity
        #[clap(short, long, value_parser)]
        verbose: bool,

        /// Show progress during file transfer
        #[clap(short, long, value_parser)]
        progress: bool,
    },
    /// Initialize link
    Init,
    /// Initialize bbup client
    Setup,
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

struct Flags {
    verbose: bool,
    progress: bool,
}
struct Connection {
    local_port: u16,
    server_port: u16,
    host_name: String,
    host_address: String,
}
struct ProcessConfig {
    link_root: PathBuf,
    exclude_list: Vec<Regex>,
    endpoint: PathBuf,
    connection: Connection,
    flags: Flags,
}
impl ProcessConfig {
    fn local_temp_path(&self) -> PathBuf {
        self.link_root.join(".bbup").join("temp")
    }
    fn lkc_path(&self) -> PathBuf {
        self.link_root.join(".bbup").join("last-known-commit.json")
    }
    fn old_tree_path(&self) -> PathBuf {
        self.link_root.join(".bbup").join("old-hash-tree.json")
    }
}
struct ProcessState {
    last_known_commit: Option<String>,
    old_tree: Option<hashtree::HashTreeNode>,
    new_tree: Option<hashtree::HashTreeNode>,
    local_delta: Option<structs::Delta>,
    update: Option<structs::Commit>,
}

impl ProcessState {
    fn new() -> ProcessState {
        ProcessState {
            last_known_commit: None,
            old_tree: None,
            new_tree: None,
            local_delta: None,
            update: None,
        }
    }
}

fn get_local_delta(config: &ProcessConfig, state: &mut ProcessState) -> Result<()> {
    if config.flags.verbose {
        println!("calculating local delta...")
    }

    state.last_known_commit = Some(fs::load(&config.lkc_path())?);

    let old_tree = fs::load(&config.old_tree_path())?;
    let new_tree = hashtree::get_hash_tree(&config.link_root, &config.exclude_list)?;
    let local_delta = hashtree::delta(&old_tree, &new_tree);

    if config.flags.verbose {
        if local_delta.len() == 0 {
            println!("local delta: no local changes to push")
        } else {
            println!("local delta:\n{}", local_delta.pretty_print(1))
        }
    }

    state.old_tree = Some(old_tree);
    state.new_tree = Some(new_tree);
    state.local_delta = Some(local_delta);
    Ok(())
}
async fn pull_update_delta<T, R>(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom<T, R>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    match &state.last_known_commit {
        Some(lkc) => {
            if config.flags.verbose {
                println!("pulling from server...")
            }
            // [PULL] Send last known commit to pull updates in case of any
            com.send_struct(lkc)
                .await
                .context("could not send last known commit")?;

            // [PULL] Get delta from last_known_commit to server's most recent commit
            let mut update: structs::Commit = com
                .get_struct()
                .await
                .context("could not get update-delta from server")?;

            // [PULL] Filter out updates that match the exclude_list
            update.delta.retain(
                |item| !match utils::to_exclude(&item.path, &config.exclude_list) {
                    Ok(val) => val,
                    Err(_) => false,
                },
            );

            if config.flags.verbose {
                if update.delta.len() == 0 {
                    println!("pull delta: no missed change to pull")
                } else {
                    println!("pull delta:\n{}", update.delta.pretty_print(1))
                }
            }

            state.update = Some(update);

            Ok(())
        }
        _ => {
            anyhow::bail!(
                "Some part of the state was required for pull-update-delta but is missing\nstate.last_known_commit: {}",
                &state.last_known_commit.is_some()
            )
        }
    }
}
async fn check_for_conflicts(state: &mut ProcessState) -> Result<()> {
    match (&state.local_delta, &state.update) {
        (
            Some(local_delta),
            Some(structs::Commit {
                commit_id: _,
                delta: update_delta,
            }),
        ) => {
            let mut conflicts: Vec<(String, String)> = Vec::new();
            local_delta.into_iter().for_each(|local_change| {
                update_delta.into_iter().for_each(|update_change| {
                    if (local_change.path.eq(&update_change.path)
                        && local_change.hash.ne(&update_change.hash))
                        || local_change.path.starts_with(&update_change.path)
                        || update_change.path.starts_with(&local_change.path)
                    {
                        // TODO: make the conflic explanation a little bit better
                        conflicts.push((
                            format!("local_change:  {:?}", local_change.path),
                            format!("update change: {:?}", update_change.path),
                        ));
                    }
                })
            });
            if conflicts.len() > 0 {
                println!("conflicts:");
                conflicts
                    .into_iter()
                    .for_each(|s| println!("\t!!! {}\n\t    {}", s.0, s.1));
                return Err(anyhow::Error::new(utils::std_err(
                    "found conflicts between pulled update and local changes. Resolve manually",
                )));
            }
            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for check-for-conflicts but is missing\nstate.local_delta: {}\nstate.update: {}",
				state.local_delta.is_some(),
				state.update.is_some(),
			)
        }
    }
}

async fn download_update<T, R>(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom<T, R>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    match &state.update {
        Some(structs::Commit {
            commit_id: _,
            delta: update_delta,
        }) => {
            for change in update_delta {
                if change.action != structs::Action::Removed
                    && change.object_type != structs::ObjectType::Dir
                {
                    com.send_struct(Some(change.path.clone())).await?;
                    com.get_file_to(&config.local_temp_path().join(change.path.clone()))
                        .await?;
                }
            }

            com.send_struct(None::<PathBuf>).await?;
            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for download-update but is missing\nstate.update: {}",
				state.update.is_some(),
			)
        }
    }
}
async fn apply_update(config: &ProcessConfig, state: &mut ProcessState) -> Result<()> {
    match (&state.update, &mut state.old_tree) {
        (
            Some(structs::Commit {
                commit_id,
                delta: update_delta,
            }),
            Some(old_tree),
        ) => {
            for change in update_delta {
                let path = config.link_root.join(&change.path);
                let from_temp_path = config.local_temp_path().join(&change.path);
                match (change.action, change.object_type) {
                    (structs::Action::Removed, structs::ObjectType::Dir) => {
                        std::fs::remove_dir(&path).context(format!(
                            "could not remove directory to apply update\npath: {:?}",
                            path
                        ))?
                    }
                    (structs::Action::Removed, _) => std::fs::remove_file(&path).context(
                        format!("could not remove file to apply update\npath: {:?}", path),
                    )?,
                    (structs::Action::Added, structs::ObjectType::Dir) => {
                        std::fs::create_dir(&path).context(format!(
                            "could not create directory to apply update\npath: {:?}",
                            path
                        ))?
                    }
                    (structs::Action::Edited, structs::ObjectType::Dir) => {
                        unreachable!("Dir cannot be edited: broken update delta")
                    }
                    (structs::Action::Added, _) | (structs::Action::Edited, _) => {
                        std::fs::rename(&from_temp_path, &path).context(format!(
                            "could not copy file from temp to apply update\npath: {:?}",
                            path
                        ))?;
                    }
                }
            }
            old_tree.apply_delta(update_delta)?;
            let new_tree = hashtree::get_hash_tree(&config.link_root, &config.exclude_list)?;
            let local_delta = hashtree::delta(&old_tree, &new_tree);

            state.new_tree = Some(new_tree);
            state.local_delta = Some(local_delta);

            fs::save(&config.old_tree_path(), &old_tree)?;
            fs::save(&config.lkc_path(), commit_id)?;

            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for apply-update but is missing\nstate.update: {}\nstate.old_tree: {}",
				state.update.is_some(),
				state.old_tree.is_some(),
			)
        }
    }
}
async fn upload_changes<T, R>(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom<T, R>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    match (&state.local_delta, &state.new_tree) {
        (Some(local_delta), Some(new_tree)) => {
            // Await green light to procede
            com.check_ok().await?;

            com.send_struct(local_delta).await?;

            loop {
                let path: Option<PathBuf> = com.get_struct().await?;
                let path = match path {
                    Some(val) => val,
                    None => break,
                };
                com.send_file_from(&config.link_root.join(path)).await?;
            }

            let new_commit_id: String = com.get_struct().await?;

            fs::save(&config.old_tree_path(), &new_tree)?;
            fs::save(&config.lkc_path(), &new_commit_id)?;

            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for upload-changes but is missing\nstate.local_delta: {}\nstate.new_tree: {}",
				state.local_delta.is_some(),
				state.new_tree.is_some(),
			)
        }
    }
}

async fn process_link(config: ProcessConfig) -> Result<()> {
    let mut tunnel = ssh_tunnel::SshTunnel::to(
        config.connection.local_port,
        config.connection.server_port,
        config.connection.host_name.clone(),
        config.connection.host_address.clone(),
    )?;

    if config.flags.verbose {
        println!("ssh tunnel PID: {}", &tunnel.pid());
    }

    tunnel.wait_for_ready()?;

    // Start connection
    let socket = TcpStream::connect(format!("127.0.0.1:{}", config.connection.local_port))
        .await
        .context("could not connect to server")?;
    let mut com = BbupCom::from_split(socket.into_split(), config.flags.progress);

    // Await green light to procede
    com.check_ok()
        .await
        .context("could not get green light from server to procede with conversation")?;

    com.send_struct(&config.endpoint).await?;

    let mut state = ProcessState::new();

    {
        // GET DELTA
        get_local_delta(&config, &mut state)?;
    }

    {
        // PULL
        com.send_struct(com::JobType::Pull).await?;
        pull_update_delta(&config, &mut state, &mut com).await?;
        check_for_conflicts(&mut state).await?;
        download_update(&config, &mut state, &mut com).await?;
        apply_update(&config, &mut state).await?;
    }

    {
        // PUSH
        com.send_struct(com::JobType::Push).await?;
        upload_changes(&config, &mut state, &mut com).await?;
    }

    // Terminate conversation with server
    com.send_struct(com::JobType::Quit).await?;

    tunnel.termiate();
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
    let cwd = std::env::current_dir()?;

    if args.cmd == SubCommand::Setup {
        if home_dir.join(".config").join("bbup-client").exists()
            && home_dir
                .join(".config")
                .join("bbup-client")
                .join("config.yaml")
                .exists()
        {
            anyhow::bail!("bbup client is already setup");
        }

        let local_port = io::get_input("enter local port (0-65535): ")?.parse::<u16>()?;
        let server_port = io::get_input("enter server port (0-65535): ")?.parse::<u16>()?;
        let host_name = io::get_input("enter host name: ")?;
        let host_address = io::get_input("enter host address: ")?;

        std::fs::create_dir_all(home_dir.join(".config").join("bbup-client"))?;
        let settings = structs::ClientSettings {
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
            &fs::ClientConfig {
                settings,
                links: Vec::new(),
            },
        )?;

        println!("bbup client set up correctly!");

        return Ok(());
    }

    let global_config: fs::ClientConfig = fs::load(
        &home_dir
            .join(".config")
            .join("bbup-client")
            .join("config.yaml"),
    )?;

    if args.cmd == SubCommand::Init {
        if cwd.join(".bbup").exists() && cwd.join(".bbup").join("config.yaml").exists() {
            anyhow::bail!(
                "Current directory [{:?}] is already initialized as a backup source",
                cwd
            )
        }
        if !cwd.join(".bbup").exists() {
            std::fs::create_dir_all(cwd.join(".bbup"))?;
        }
        let endpoint = PathBuf::from(io::get_input("set endpoint (relative to archive root): ")?);
        let add_exclude_list = io::get_input("add exclude list [y/N]?: ")?;
        let mut exclude_list: Vec<String> = Vec::new();
        if add_exclude_list.to_ascii_lowercase().eq("y")
            || add_exclude_list.to_ascii_lowercase().eq("yes")
        {
            println!("add regex rules in string form. To stop, enter empty string");
            loop {
                let rule = io::get_input("rule: ")?;
                if rule.eq("") {
                    break;
                }
                exclude_list.push(rule);
            }
        }
        let local_config = fs::LinkConfig {
            link_type: structs::LinkType::Bijection,
            endpoint,
            exclude_list: exclude_list.clone(),
        };

        let mut exclude_list_regex: Vec<Regex> = Vec::new();
        exclude_list_regex.push(Regex::new("\\.bbup/").unwrap());
        for rule in &exclude_list {
            exclude_list_regex.push(Regex::new(&rule).context(
                "could not generate regex from pattern from exclude_list in link config",
            )?);
        }
        fs::save(&cwd.join(".bbup").join("config.yaml"), &local_config)?;
        let tree = hashtree::get_hash_tree(&cwd, &exclude_list_regex)?;
        fs::save(&cwd.join(".bbup").join("old-hash-tree.json"), &tree)?;
        fs::save(
            &cwd.join(".bbup").join("last-known-commit.json"),
            &String::from("0").repeat(64),
        )?;

        println!("backup source initialized correctly!");

        return Ok(());
    }

    match args.cmd {
        SubCommand::Sync { verbose, progress } => {
            if !cwd.join(".bbup").exists() || !cwd.join(".bbup").join("config.yaml").exists() {
                anyhow::bail!(
                    "Current directory [{:?}] isn't initialized as a backup source",
                    cwd
                )
            }

            // Parse Link configs
            let local_config: fs::LinkConfig = fs::load(&cwd.join(".bbup").join("config.yaml"))?;

            let mut exclude_list: Vec<Regex> = Vec::new();
            exclude_list.push(Regex::new("\\.bbup/").unwrap());
            for rule in &local_config.exclude_list {
                exclude_list.push(Regex::new(&rule).context(
                    "could not generate regex from pattern from exclude_list in link config",
                )?);
            }

            let connection = Connection {
                local_port: global_config.settings.local_port,
                server_port: global_config.settings.server_port,
                host_name: global_config.settings.host_name.clone(),
                host_address: global_config.settings.host_address.clone(),
            };
            let flags = Flags { verbose, progress };
            let config = ProcessConfig {
                link_root: cwd.clone(),
                exclude_list,
                endpoint: local_config.endpoint,
                connection,
                flags,
            };

            if verbose {
                println!("Syncing link: [{:?}]", cwd);
            }
            match process_link(config).await {
                Ok(()) => {
                    if verbose {
                        println!("Link correctly synced: [{:?}] ", cwd);
                    }
                }
                Err(err) => {
                    bail!("Failed to sync link [{:?}]\n{:?}", cwd, err);
                }
            };
        }
        _ => { /* already handled */ }
    }

    Ok(())
}
