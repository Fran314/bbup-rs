use std::{path::PathBuf, sync::Arc};

use bbup_rust::{fs, structs, random, io, com};
// use bbup_rust::comunications::{BbupRead, BbupWrite};

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use tokio::{
	sync::Mutex,
    net::{TcpListener, TcpStream},
};

struct ServerState {
	home_dir: PathBuf,
	archive_root: PathBuf,
	server_port: u16,
    commit_list: fs::CommitList,
}

impl ServerState {
	pub fn load(home_dir: PathBuf) -> Result<ServerState> {
		let config: fs::ServerConfig = fs::load(&home_dir.join(".config").join("bbup-server").join("config.yaml"))?;

		// Load server state, necessary for conversation and
		//	"shared" between tasks (though only one can use it
		//	at a time and those who can't have it terminate)
		let commit_list: fs::CommitList =
			fs::load(&home_dir.join(".config").join("bbup-server").join("commit-list.json"))?;
		Ok(ServerState {
			home_dir: home_dir,
			archive_root: config.archive_root,
			server_port: config.server_port,
			commit_list
		})
	}

	pub fn save(&mut self) -> Result<()> {
		fs::save(
			&self.home_dir.join(".config").join("bbup-server").join("config.yaml"),
			&fs::ServerConfig { server_port: self.server_port, archive_root: self.archive_root.clone()}
		)?;
		fs::save(
			&self.home_dir.join(".config").join("bbup-server").join("commit-list.json"),
			&self.commit_list
		)?;

		Ok(())
	}
}

#[derive(Subcommand, Debug, PartialEq)]
enum SubCommand {
    /// Run the daemon
    Run {
		/// Show progress during file transfer
		#[clap(short, long)]
		progress: bool
	},
    /// Initialize bbup client
    Setup,
}

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(long, value_parser)]
    home_dir: Option<PathBuf>,

    #[clap(subcommand)]
    cmd: SubCommand,
}

fn merge_delta(main: &mut structs::Delta, prec: &structs::Delta) {
    for prec_change in prec {
        match main
            .into_iter()
            .position(|el| el.path.eq(&prec_change.path) && el.object_type.eq(&prec_change.object_type))
        {
            None => main.push(prec_change.clone()),
            Some(pos) => {
				let succ_change = main[pos].clone();
                match (prec_change.action, succ_change.action) {
                    (structs::Action::Added, structs::Action::Added)
                    | (structs::Action::Edited, structs::Action::Added)
					| (structs::Action::Removed, structs::Action::Edited)
					| (structs::Action::Removed, structs::Action::Removed)
						=> unreachable!("case is unreachable as long as main and precedent commit are compatible and correct"),

                    (structs::Action::Added, structs::Action::Edited)
						// If object is added and later on edited, it's the same as adding it with the new content (succ hash)
						=> main[pos] = structs::Change::new(
							structs::Action::Added, 
							succ_change.object_type.clone(), 
							succ_change.path.clone(), 
							succ_change.hash.clone()
						),
                    (structs::Action::Added, structs::Action::Removed) 
						// If object is added and later on removed, it's the same as not mentioning it at all
						=> { main.remove(pos); },
                    (structs::Action::Edited, structs::Action::Edited)
						// If object is edited twice, it's the same as editing it once with the new content (succ hash)
						=> main[pos] = structs::Change::new(
							structs::Action::Edited,
							succ_change.object_type.clone(),
							succ_change.path.clone(),
							succ_change.hash.clone()
						),
                    (structs::Action::Edited, structs::Action::Removed)
						// If object is edited and later on removed, it's the same as just removing it
						=> main[pos] = structs::Change::new(
							structs::Action::Removed,
							succ_change.object_type.clone(),
							succ_change.path.clone(),
							None
						),
                    (structs::Action::Removed, structs::Action::Added)
						// If object is removed and later on added, it's the same as editing it with the new content (succ hash)
						=> main[pos] = structs::Change::new(
							structs::Action::Edited,
							succ_change.object_type.clone(),
							succ_change.path.clone(),
							succ_change.hash.clone()
						),
                }
			},
        }
    }
}

fn get_update_delta(commit_list: &fs::CommitList, endpoint: &PathBuf, lkc: String) -> structs::Delta {
	let mut output: structs::Delta = Vec::new();
	for commit in commit_list.into_iter().rev() {
		if commit.commit_id.eq(&lkc)
		{
			break;
		}
		merge_delta(&mut output, &commit.delta);
	}
	output.iter().filter_map(|change| match change.path.strip_prefix(<PathBuf as AsRef<std::path::Path>>::as_ref(&endpoint)) {
		Ok(val) => Some(structs::Change {path: val.to_path_buf(), ..change.clone() }),
		Err(_) => None
	}).collect()
}

#[tokio::main]
async fn main() -> Result<()> {
	// Parse command line arguments
    let args = Args::parse();
    let home_dir = match args.home_dir {
        Some(val) => Some(val),
        None => dirs::home_dir(),
    }.context("could not resolve home_dir path")?;

	if args.cmd == SubCommand::Setup {
		if home_dir.join(".config").join("bbup-server").exists()
            && home_dir
                .join(".config")
                .join("bbup-server")
                .join("config.yaml")
                .exists()
        {
            anyhow::bail!("bbup server is already setup");
        }

        std::fs::create_dir_all(home_dir.join(".config").join("bbup-server"))?;
        let server_port = io::get_input("enter server port (0-65535): ")?.parse::<u16>()?;
		let archive_root = PathBuf::from(io::get_input("enter archive root (relative to ~): ")?);
		fs::save(&home_dir.join(".config").join("bbup-server").join("config.yaml"), &fs::ServerConfig { server_port, archive_root: archive_root.clone() })?;

		let mut commit_list: fs::CommitList = Vec::new();
		commit_list.push(structs::Commit { commit_id: String::from("0").repeat(64), delta: Vec::new() });
		fs::save(&home_dir.join(".config").join("bbup-server").join("commit-list.json"), &commit_list)?;

		std::fs::create_dir_all(home_dir.join(&archive_root).join(".bbup").join("temp"))?;

        println!("bbup server set up correctly!");

		return Ok(());
	}

	// Load server state, necessary for conversation and
	//	"shared" between tasks (though only one can use it
	//	at a time and those who can't have it terminate)
	let state = ServerState::load(home_dir)?;
    let listener = TcpListener::bind(format!("127.0.0.1:{}", state.server_port)).await?;
    let state = Arc::new(Mutex::new(state));

	match args.cmd {
		SubCommand::Run { progress } => {
			// Start TCP server and spawn a task for each connection
			loop {
				let (socket, _) = listener.accept().await?;
				let state = state.clone();
				tokio::spawn(async move {
					match process(socket, state, progress).await {
						Ok(()) => println!("connection processed correctly"),
						Err(err) => println!("Error: {err:?}"),
					}
				});
			}
		},
		_ => { /* already handled */}
	}
    Ok(())
}

async fn process(socket: TcpStream, state: Arc<Mutex<ServerState>>, progress: bool) -> Result<()> {
	let mut com = com::BbupCom::from_split(socket.into_split(), progress);

	// Try to lock state and get conversation privilege
    let mut state = match state.try_lock() {
        Ok(val) => val,
        Err(_) => {
			// Could not get conversation privilege, deny conversation
			//	and terminate stream
			com.send_error(1, "server occupied")
            .await?;
            return Ok(());
        }
    };
	// Reply with green light to conversation, send OK
	com.send_ok().await.context("could not send greenlight for conversation")?;

	let endpoint: PathBuf = com.get_struct().await.context("could not get backup endpoint")?;

	loop {
		let jt: com::JobType = com.get_struct().await.context("could not get job type")?;
		match jt {
			com::JobType::Quit => { 
				com.send_ok().await?;
				break
			},
			com::JobType::Pull => {
				let last_known_commit: String = com.get_struct().await.context("could not get lkc")?;
			
				// calculate update for client
				let delta = get_update_delta(&state.commit_list, &endpoint, last_known_commit);
			
				// send update delta to client for pull
				com.send_struct(
					structs::Commit { 
						commit_id: state.commit_list[state.commit_list.len() - 1].commit_id.clone(), 
						delta 
					}
				).await.context("could not send update delta")?;
			
				// send all files requested by client
				loop {
					let path: Option<PathBuf> = com.get_struct().await.context("could not get path for file to send")?;
					let path = match path {
						Some(val) => val,
						None => break
					};
					com.send_file_from(&state.home_dir.join(&state.archive_root).join(&endpoint).join(&path))
						.await
						.context(
							format!(
								"could not send file at path: {:?}", 
								&state.home_dir.join(&state.archive_root).join(&endpoint).join(&path)
							)
						)?;
				}
			}
			com::JobType::Push => {
				std::fs::create_dir_all(state.home_dir.join(&state.archive_root).join(".bbup").join("temp"))?;
				std::fs::remove_dir_all(state.home_dir.join(&state.archive_root).join(".bbup").join("temp"))?;
				std::fs::create_dir(state.home_dir.join(&state.archive_root).join(".bbup").join("temp"))?;

				// Reply with green light for push
				com.send_ok().await.context("could not send greenlight for push")?;
			
				let local_delta: structs::Delta = com.get_struct().await.context("could not get delta from client")?;
			
				for change in &local_delta {
					if change.action != structs::Action::Removed
						&& change.object_type != structs::ObjectType::Dir
					{
						com.send_struct(Some(change.path.clone())).await.context(
							format!(
								"could not send path for file to send at path: {:?}", 
								&change.path.clone()
							)
						)?;
						com.get_file_to(
							&state
								.home_dir
								.join(&state.archive_root)
								.join(".bbup")
								.join("temp")
								.join(change.path.clone()),
						)
						.await
						.context(
							format!(
								"could not get file at path: {:?}", 
								&state
									.home_dir
									.join(&state.archive_root)
									.join(".bbup")
									.join("temp")
									.join(change.path.clone())
							)
						)?;
					}
				}
				com.send_struct(None::<PathBuf>).await.context("could not send empty path to signal end of file transfer")?;
			
				for change in &local_delta {
					let path = state.home_dir.join(&state.archive_root).join(&endpoint).join(&change.path);
					let from_temp_path = state
						.home_dir
						.join(&state.archive_root)
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
							std::fs::rename(&from_temp_path, &path).context(format!(
								"could not copy file from temp to apply update\npath: {:?}",
								path
							))?;
						}
					}
				}
			
				let local_delta: structs::Delta = local_delta.into_iter().map(|change| structs::Change { path: endpoint.join(&change.path), ..change}).collect();
				let commit_id = random::random_hex(64);
				state.commit_list.push(structs::Commit { commit_id: commit_id.clone(), delta: local_delta });
				state.save().context("could not save push update")?;
			
				com.send_struct(commit_id).await.context("could not send commit id for the push")?;
			}
		}
	}
	
    Ok(())
}
