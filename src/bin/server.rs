use std::{path::PathBuf, sync::Arc};

use bbup_rust::{fs, structs, random};
use bbup_rust::comunications::{BbupRead, BbupWrite};

use anyhow::{Result, Context};
use clap::Parser;
use tokio::{
	sync::Mutex,
    net::{TcpListener, TcpStream},
};

struct ServerState {
	home_dir: PathBuf,
	archive_root: PathBuf,
    commit_list: fs::CommitList,
}

impl ServerState {
	pub fn load(home_dir: PathBuf) -> Result<ServerState> {
		let config: fs::ServerConfing = fs::load(&home_dir.join(".config").join("bbup-server").join("config.yaml"))?;

		// Load server state, necessary for conversation and
		//	"shared" between tasks (though only one can use it
		//	at a time and those who can't have it terminate)
		let commit_list: fs::CommitList =
			fs::load(&home_dir.join(".config").join("bbup-server").join("commit-list.json"))?;
		Ok(ServerState {
			home_dir: home_dir,
			archive_root: config.archive_root,
			commit_list
		})
	}

	pub fn save(&mut self) -> Result<()> {
		fs::save(
			&self.home_dir.join(".config").join("bbup-server").join("config.yaml"),
			&fs::ServerConfing { archive_root: self.archive_root.clone()}
		)?;
		fs::save(
			&self.home_dir.join(".config").join("bbup-server").join("commit-list.json"),
			&self.commit_list
		)?;

		Ok(())
	}
}

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(short, long, value_parser)]
    dir: Option<PathBuf>,
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
    let home_dir = match args.dir {
        Some(val) => Some(val),
        None => dirs::home_dir(),
    }.context("could not resolve home_dir path")?;

	// Load server state, necessary for conversation and
	//	"shared" between tasks (though only one can use it
	//	at a time and those who can't have it terminate)
    let state = Arc::new(Mutex::new(ServerState::load(home_dir)?));

	// Start TCP server and spawn a task for each connection
    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    loop {
        let (socket, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            process(socket, state).await.unwrap();
        });
    }
}

async fn process(socket: TcpStream, state: Arc<Mutex<ServerState>>) -> Result<()> {
    let (mut rx, mut tx) = socket.into_split();

	// Try to lock state and get conversation privilege
    let mut state = match state.try_lock() {
        Ok(val) => val,
        Err(_) => {
			// Could not get conversation privilege, deny conversation
			//	and terminate stream
			tx.send_error(1, "bbup-server, server occupied")
            .await?;
            return Ok(());
        }
    };

	// Reply with green light to conversation, send OK
	tx.send_ok().await?;

	let endpoint: PathBuf = rx.get_struct().await?;

	// [Client-PULL] recieve last known commit from CLIENT
    let last_known_commit: String = rx.get_struct().await?;

	// [Client-PULL] calculate update for client
	let delta = get_update_delta(&state.commit_list, &endpoint, last_known_commit);

	// [Client-PULL] send update to client for pull
    tx.send_struct(
		structs::ClientUpdate { 
			root: state.home_dir.join(&state.archive_root).join(&endpoint), 
			commit_id: state.commit_list[0].commit_id.clone(), 
			delta 
		}
    ).await?;

	loop {
		let path: Option<PathBuf> = rx.get_struct().await?;
		let path = match path {
			Some(val) => val,
			None => break
		};
		tx.send_file_from(&state.home_dir.join(&state.archive_root).join(&endpoint).join(path)).await?;
	}


	std::fs::remove_dir_all(state.home_dir.join(&state.archive_root).join(".bbup").join("temp"))?;
	std::fs::create_dir(state.home_dir.join(&state.archive_root).join(".bbup").join("temp"))?;
	// Reply with green light to conversation, send OK
	tx.send_ok().await?;

	let local_delta: structs::Delta = rx.get_struct().await?;

	for change in &local_delta {
        if change.action != structs::Action::Removed
            && change.object_type != structs::ObjectType::Dir
        {
            tx.send_struct(Some(change.path.clone())).await?;
            rx.get_file_to(
                &state
					.home_dir
					.join(&state.archive_root)
                    .join(".bbup")
                    .join("temp")
                    .join(change.path.clone()),
            )
            .await?;
        }
	}
    tx.send_struct(None::<PathBuf>).await?;

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
                std::fs::copy(&from_temp_path, &path).context(format!(
                    "could not copy file from temp to apply update\npath: {:?}",
                    path
                ))?;
            }
        }
    }

	let local_delta: structs::Delta = local_delta.into_iter().map(|change| structs::Change { path: endpoint.join(&change.path), ..change}).collect();
	let commit_id = random::random_hex(64);
	state.commit_list.push(structs::Commit { commit_id: commit_id.clone(), delta: local_delta });
	state.save()?;

	tx.send_struct(commit_id).await?;


	// Rest of protocol


    Ok(())
}
