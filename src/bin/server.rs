use std::{path::PathBuf, sync::Arc};

use bbup_rust::{comunications as com, fs, structs};
use bbup_rust::comunications::{BbupRead, BbupWrite};

use anyhow::{Result, Context};
use clap::Parser;
use tokio::{
	sync::Mutex,
    net::{TcpListener, TcpStream},
};

struct ServerState {
	home_dir: PathBuf,
	config: fs::ServerConfing,
    commit_list: fs::CommitList,
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
						=> main[pos] = structs::Change::new(structs::Action::Added, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone()),
                    (structs::Action::Added, structs::Action::Removed) 
						// If object is added and later on removed, it's the same as not mentioning it at all
						=> { main.remove(pos); },
                    (structs::Action::Edited, structs::Action::Edited)
						// If object is edited twice, it's the same as editing it once with the new content (succ hash)
						=> main[pos] = structs::Change::new(structs::Action::Edited, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone()),
                    (structs::Action::Edited, structs::Action::Removed)
						// If object is edited and later on removed, it's the same as just removing it
						=> main[pos] = structs::Change::new(structs::Action::Removed, succ_change.object_type.clone(), succ_change.path.clone(), None),
                    (structs::Action::Removed, structs::Action::Added)
						// If object is removed and later on added, it's the same as editing it with the new content (succ hash)
						=> main[pos] = structs::Change::new(structs::Action::Edited, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone()),
                }
			},
        }
    }
}

fn get_update_delta(commit_list: &fs::CommitList, update_request: &structs::UpdateRequest) -> structs::Delta {
	let mut output: structs::Delta = Vec::new();
	for commit in commit_list.into_iter() {
		if commit.commit_id.eq(&update_request.lkc)
		{
			break;
		}
		merge_delta(&mut output, &commit.delta);
	}
	output.iter().filter_map(|change| match change.path.strip_prefix(<PathBuf as AsRef<std::path::Path>>::as_ref(&update_request.endpoint)) {
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

	fs::save(&home_dir.join(".config").join("bbup-server").join("config.yaml"), &fs::ServerConfing { archive_root: PathBuf::from("server-archive")})?;
	let config: fs::ServerConfing = fs::load(&home_dir.join(".config").join("bbup-server").join("config.yaml"))?;

	// Load server state, necessary for conversation and
	//	"shared" between tasks (though only one can use it
	//	at a time and those who can't have it terminate)
    let commit_list: fs::CommitList =
        fs::load(&home_dir.join(".config").join("bbup-server").join("commit-list.json"))?;
    let state = Arc::new(Mutex::new(ServerState {
		home_dir,
		config,
        commit_list
    }));

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

async fn process(socket: TcpStream, state: Arc<Mutex<ServerState>>) -> std::io::Result<()> {
    let (mut rx, mut tx) = socket.into_split();

	// Try to lock state and get conversation privilege
    let state = match state.try_lock() {
        Ok(val) => val,
        Err(_) => {
			// Could not get conversation privilege, deny conversation
			//	and terminate stream
            tx.send_struct(1, com::Empty, "bbup-server, server occupied")
            .await?;
            return Ok(());
        }
    };

	// // Reply with green light to conversation, send status 0 (OK)
    tx.send_struct(0, com::Empty, "bbup-server, procede with last known commit").await?;

	// [Client-PULL] recieve last known commit from CLIENT
    let update_request: structs::UpdateRequest = rx.get_struct().await?;

	// // [Client-PULL] calculate update for client
	let delta = get_update_delta(&state.commit_list, &update_request);

	// // [Client-PULL] send update to client for pull
    tx.send_struct(
		0,
		structs::ClientUpdate { 
			root: state.home_dir.join(&state.config.archive_root).join(&update_request.endpoint), 
			commit_id: state.commit_list[0].commit_id.clone(), 
			delta 
		},
		"update_delta since last known commit"
    ).await?;


	// Rest of protocol


    Ok(())
}
