use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};

use clap::Parser;

use std::path::PathBuf;

use bbup_rust::{comunications as com, fs};

struct ServerState {
    commit_list: fs::CommitList,
}

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(short, long, value_parser)]
    dir: Option<PathBuf>,
}

// fn merge_delta(main: &fs::Delta, prec: &fs::Delta) -> fs::Delta {
//     let mut output = main.clone();
//     for succ_change in main {
//         match prec.into_iter().find(|el| {
//             el.path.eq(&succ_change.path) && el.object_type.eq(&succ_change.object_type)
//         }) {
//             None => output.push(succ_change.clone()),
//             _ => {}
//         }
//     }
//     for prec_change in prec {
//         match main
//             .into_iter()
//             .find(|el| el.path.eq(&prec_change.path) && el.object_type.eq(&prec_change.object_type))
//         {
//             None => output.push(prec_change.clone()),
//             Some(succ_change) => 
//                 match (prec_change.action, succ_change.action) {
//                     (fs::Action::Added, fs::Action::Added)
//                     | (fs::Action::Edited, fs::Action::Added)
// 					| (fs::Action::Removed, fs::Action::Edited)
// 					| (fs::Action::Removed, fs::Action::Removed)
// 						=> unreachable!("case is unreachable as long as main and precedent commit are compatible and correct"),

//                     (fs::Action::Added, fs::Action::Edited)
// 						// If object is added and later on edited, it's the same as adding it with the new content (succ hash)
// 						=> output.push(fs::Change::new(fs::Action::Added, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone())),
//                     (fs::Action::Added, fs::Action::Removed) 
// 						// If object is added and later on removed, it's the same as not mentioning it at all
// 						=> {},
//                     (fs::Action::Edited, fs::Action::Edited)
// 						// If object is edited twice, it's the same as editing it once with the new content (succ hash)
// 						=> output.push(fs::Change::new(fs::Action::Edited, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone())),
//                     (fs::Action::Edited, fs::Action::Removed)
// 						// If object is edited and later on removed, it's the same as just removing it
// 						=> output.push(fs::Change::new(fs::Action::Removed, succ_change.object_type.clone(), succ_change.path.clone(), None)),
//                     (fs::Action::Removed, fs::Action::Added)
// 						// If object is removed and later on added, it's the same as editing it with the new content (succ hash)
// 						=> output.push(fs::Change::new(fs::Action::Edited, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone())),
//                 },
//         }
//     }
//     output
// }


fn merge_delta(main: &mut fs::Delta, prec: &fs::Delta) {
    for prec_change in prec {
        match main
            .into_iter()
            .position(|el| el.path.eq(&prec_change.path) && el.object_type.eq(&prec_change.object_type))
        {
            None => main.push(prec_change.clone()),
            Some(pos) => {
				let succ_change = main[pos].clone();
                match (prec_change.action, succ_change.action) {
                    (fs::Action::Added, fs::Action::Added)
                    | (fs::Action::Edited, fs::Action::Added)
					| (fs::Action::Removed, fs::Action::Edited)
					| (fs::Action::Removed, fs::Action::Removed)
						=> unreachable!("case is unreachable as long as main and precedent commit are compatible and correct"),

                    (fs::Action::Added, fs::Action::Edited)
						// If object is added and later on edited, it's the same as adding it with the new content (succ hash)
						=> main[pos] = fs::Change::new(fs::Action::Added, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone()),
                    (fs::Action::Added, fs::Action::Removed) 
						// If object is added and later on removed, it's the same as not mentioning it at all
						=> { main.remove(pos); },
                    (fs::Action::Edited, fs::Action::Edited)
						// If object is edited twice, it's the same as editing it once with the new content (succ hash)
						=> main[pos] = fs::Change::new(fs::Action::Edited, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone()),
                    (fs::Action::Edited, fs::Action::Removed)
						// If object is edited and later on removed, it's the same as just removing it
						=> main[pos] = fs::Change::new(fs::Action::Removed, succ_change.object_type.clone(), succ_change.path.clone(), None),
                    (fs::Action::Removed, fs::Action::Added)
						// If object is removed and later on added, it's the same as editing it with the new content (succ hash)
						=> main[pos] = fs::Change::new(fs::Action::Edited, succ_change.object_type.clone(), succ_change.path.clone(), succ_change.hash.clone()),
                }
			},
        }
    }
}

fn get_delta_from_last_known_commit(commit_list: &fs::CommitList, last_known_commit: &String) -> fs::Delta {
	let mut output: fs::Delta = Vec::new();
	for commit in commit_list.into_iter() {
		if commit.commit_id.eq(last_known_commit)
		{
			break;
		}

		merge_delta(&mut output, &commit.delta);
	}
	output
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let home_dir = match args.dir {
        Some(val) => val,
        None => dirs::home_dir().expect("could not get home directory"),
    };

    let commit_list: fs::CommitList =
        fs::load_json(&home_dir.join(".bbup-server").join("commit-list.json"))?;

    let state = Arc::new(Mutex::new(ServerState {
        commit_list: commit_list,
    }));

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
    let mut socket = BufReader::new(socket);
    let mut buffer = String::new();

    let state = match state.try_lock() {
        Ok(val) => val,
        Err(_) => {
            com::asyncrw::write(
                &mut socket,
                com::Basic::new(1, "bbup-server, server occupied"),
            )
            .await?;
            return Ok(());
        }
    };

    com::asyncrw::write(
        &mut socket,
        com::Basic::new(0, "bbup-server, procede with last known commit"),
    )
    .await?;

    let read_val: com::LastCommit = com::asyncrw::read(&mut socket, &mut buffer).await?;

	let delta = get_delta_from_last_known_commit(&state.commit_list, &read_val.commit_id);

    com::asyncrw::write(
        &mut socket,
		fs::Commit { commit_id: state.commit_list[0].commit_id.clone(), delta }
    )
    .await?;

    Ok(())
}
