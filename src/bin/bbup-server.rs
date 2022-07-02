use std::{path::PathBuf, sync::Arc};

use bbup_rust::{com, fs, io, path::AbstractPath, random, structs};
// use bbup_rust::comunications::{BbupRead, BbupWrite};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

struct ServerState {
    home_dir: PathBuf,
    archive_root: PathBuf,
    server_port: u16,
    commit_list: fs::CommitList,
}

impl ServerState {
    pub fn load(home_dir: PathBuf) -> Result<ServerState> {
        let config: fs::ServerConfig = fs::load(
            &home_dir
                .join(".config")
                .join("bbup-server")
                .join("config.yaml"),
        )?;

        // Load server state, necessary for conversation and
        //	"shared" between tasks (though only one can use it
        //	at a time and those who can't have it terminate)
        let commit_list: fs::CommitList = fs::load(
            &home_dir
                .join(".config")
                .join("bbup-server")
                .join("commit-list.json"),
        )?;
        Ok(ServerState {
            home_dir: home_dir,
            archive_root: config.archive_root,
            server_port: config.server_port,
            commit_list,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        fs::save(
            &self
                .home_dir
                .join(".config")
                .join("bbup-server")
                .join("config.yaml"),
            &fs::ServerConfig {
                server_port: self.server_port,
                archive_root: self.archive_root.clone(),
            },
        )?;
        fs::save(
            &self
                .home_dir
                .join(".config")
                .join("bbup-server")
                .join("commit-list.json"),
            &self.commit_list,
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
        progress: bool,
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
            .position(|change| change.path.eq(&prec_change.path))
        {
            None => main.push(prec_change.clone()),
            Some(pos) => {
                let succ_change = main[pos].clone();
                match (
                    prec_change.change_type.clone(),
                    succ_change.change_type.clone(),
                ) {
                    (structs::ChangeType::Added(_), structs::ChangeType::Added(_))
                    | (structs::ChangeType::Edited(_), structs::ChangeType::Added(_))
                    | (structs::ChangeType::Removed(_), structs::ChangeType::Edited(_))
                    | (structs::ChangeType::Removed(_), structs::ChangeType::Removed(_)) => {
                        panic!("Commit list is broken! Succession of incompatible changes is saved in the commit list\nat path: {:?}\nchange {:?} occurred after previous change {:?}, and these are incompatible", prec_change.path, succ_change.change_type, prec_change.change_type)
                    }

                    // If object is added and later on edited, it's the same as adding it with the new content (hash1)
                    (structs::ChangeType::Added(add0), structs::ChangeType::Edited(edit1)) => {
                        let add = match (add0, edit1) {
                            (
                                structs::Adding::FileType(type0, _),
                                structs::Editing::FileType(type1, hash1),
                            ) if type0 == type1 => structs::Adding::FileType(type1, hash1),
                            _ => panic!("Commit list is broken! Succession of incompatible changes is saved in the commit list\nentry type mismatch for path: {:?}", succ_change.path),
                        };
                        main[pos] = structs::Change {
                            path: succ_change.path.clone(),
                            change_type: structs::ChangeType::Added(add),
                        }
                    }

                    // If object is added and later on removed, it's the same as not mentioning it at all
                    (structs::ChangeType::Added(_), structs::ChangeType::Removed(_)) => {
                        main.remove(pos);
                    }

                    // If object is edited twice, it's the same as editing it once with the new content (succ hash)
                    // That said, because a double edit results in an edit containing the most recent hash value,
                    //	and main[pos] is already the an edit containing the most recent hash value, merging these
                    //	two changes means doing absolutely nothing, hence why we're doing nothing in this branch
                    // Basically the same happens when a removal happens after an edit. An edit succeded by a
                    //	removal results in only a removal, and main[pos] is already such a removal
                    (structs::ChangeType::Edited(_), structs::ChangeType::Edited(_)) => { /* Do nothing */
                    }
                    (structs::ChangeType::Edited(_), structs::ChangeType::Removed(_)) => { /* Do nothing */
                    }

                    // If object is removed and later on added, we have three cases:
                    //	- (A) The entry types of the removed entry and the added entry match, and they're both a dir
                    //		In this case, it's the same as just doing nothing at all
                    //	- (B) The entry types match, and they're something else (file or symlink)
                    //		In this case, it's the same as editing the object with the hash derived from the addition
                    //	- (C) The entry types do not match
                    //		In this case we just have both the removal of the old object and the
                    //		addition of the new object. Because the addition is already in main, we only have to add
                    //		insert the removal in main
                    (structs::ChangeType::Removed(remove0), structs::ChangeType::Added(add0)) => {
                        match (remove0, add0) {
                            // Case (A)
                            (structs::Removing::Dir, structs::Adding::Dir) => { /* Do nothing */ }

                            // Case (B)
                            (
                                structs::Removing::FileType(type0),
                                structs::Adding::FileType(type1, hash1),
                            ) if type0 == type1 => {
                                let edit = structs::Editing::FileType(type1, hash1);
                                main[pos] = structs::Change {
                                    path: succ_change.path.clone(),
                                    change_type: structs::ChangeType::Edited(edit),
                                }
                            }

                            // Case (C)
                            _ => {
                                main.push(prec_change.clone());
                            }
                        }
                    }
                }
                // match (prec_change.action, succ_change.action) {
                //     // (structs::Action::Added, structs::Action::Added)
                //     // | (structs::Action::Edited, structs::Action::Added)
                // 	// | (structs::Action::Removed, structs::Action::Edited)
                // 	// | (structs::Action::Removed, structs::Action::Removed)
                // 	// 	=> unreachable!("case is unreachable as long as main and precedent commit are compatible and correct"),

                //     // (structs::Action::Added, structs::Action::Edited)
                // 	// 	// If object is added and later on edited, it's the same as adding it with the new content (succ hash)
                // 	// 	=> main[pos] = structs::Change::new(
                // 	// 		structs::Action::Added,
                // 	// 		succ_change.object_type.clone(),
                // 	// 		succ_change.path.clone(),
                // 	// 		succ_change.hash.clone()
                // 	// 	),
                //     // (structs::Action::Added, structs::Action::Removed)
                // 	// 	// If object is added and later on removed, it's the same as not mentioning it at all
                // 	// 	=> { main.remove(pos); },
                //     // (structs::Action::Edited, structs::Action::Edited)
                // 	// 	// If object is edited twice, it's the same as editing it once with the new content (succ hash)
                // 	// 	=> main[pos] = structs::Change::new(
                // 	// 		structs::Action::Edited,
                // 	// 		succ_change.object_type.clone(),
                // 	// 		succ_change.path.clone(),
                // 	// 		succ_change.hash.clone()
                // 	// 	),
                //     // (structs::Action::Edited, structs::Action::Removed)
                // 	// 	// If object is edited and later on removed, it's the same as just removing it
                // 	// 	=> main[pos] = structs::Change::new(
                // 	// 		structs::Action::Removed,
                // 	// 		succ_change.object_type.clone(),
                // 	// 		succ_change.path.clone(),
                // 	// 		None
                // 	// 	),
                //     (structs::Action::Removed, structs::Action::Added)
                // 		// If object is removed and later on added, it's the same as editing it with the new content (succ hash)
                // 		=> main[pos] = structs::Change::new(
                // 			structs::Action::Edited,
                // 			succ_change.object_type.clone(),
                // 			succ_change.path.clone(),
                // 			succ_change.hash.clone()
                // 		),
                // }
            }
        }
    }
}

fn get_update_delta(
    commit_list: &fs::CommitList,
    endpoint: &AbstractPath,
    lkc: String,
) -> structs::Delta {
    let mut output: structs::Delta = Vec::new();
    for commit in commit_list.into_iter().rev() {
        if commit.commit_id.eq(&lkc) {
            break;
        }
        merge_delta(&mut output, &commit.delta);
    }
    output
        .iter()
        .filter_map(|change| match change.path.strip_prefix(endpoint) {
            Ok(val) => Some(structs::Change {
                path: val,
                change_type: change.change_type.clone(),
            }),
            Err(_) => None,
        })
        .collect()
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
        fs::save(
            &home_dir
                .join(".config")
                .join("bbup-server")
                .join("config.yaml"),
            &fs::ServerConfig {
                server_port,
                archive_root: archive_root.clone(),
            },
        )?;

        let mut commit_list: fs::CommitList = Vec::new();
        commit_list.push(structs::Commit {
            commit_id: String::from("0").repeat(64),
            delta: Vec::new(),
        });
        fs::save(
            &home_dir
                .join(".config")
                .join("bbup-server")
                .join("commit-list.json"),
            &commit_list,
        )?;

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
        }
        _ => { /* already handled */ }
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
            com.send_error(1, "server occupied").await?;
            return Ok(());
        }
    };
    // Reply with green light to conversation, send OK
    com.send_ok()
        .await
        .context("could not send greenlight for conversation")?;

    let endpoint: AbstractPath = com
        .get_struct()
        .await
        .context("could not get backup endpoint")?;

    loop {
        let jt: com::JobType = com.get_struct().await.context("could not get job type")?;
        match jt {
            com::JobType::Quit => {
                com.send_ok().await?;
                break;
            }
            com::JobType::Pull => {
                let last_known_commit: String =
                    com.get_struct().await.context("could not get lkc")?;

                // calculate update for client
                let delta = get_update_delta(&state.commit_list, &endpoint, last_known_commit);

                // send update delta to client for pull
                com.send_struct(structs::Commit {
                    commit_id: state.commit_list[state.commit_list.len() - 1]
                        .commit_id
                        .clone(),
                    delta,
                })
                .await
                .context("could not send update delta")?;

                // send all files requested by client
                loop {
                    let path: Option<AbstractPath> = com
                        .get_struct()
                        .await
                        .context("could not get path for file to send")?;
                    let path = match path {
                        Some(val) => val,
                        None => break,
                    };

                    let full_path = state
                        .home_dir
                        .join(&state.archive_root)
                        .join(&endpoint.to_path_buf())
                        .join(&path.to_path_buf());

                    com.send_file_from(&full_path)
                        .await
                        .context(format!("could not send file at path: {full_path:?}"))?;
                }
            }
            com::JobType::Push => {
                std::fs::create_dir_all(
                    state
                        .home_dir
                        .join(&state.archive_root)
                        .join(".bbup")
                        .join("temp"),
                )?;
                std::fs::remove_dir_all(
                    state
                        .home_dir
                        .join(&state.archive_root)
                        .join(".bbup")
                        .join("temp"),
                )?;
                std::fs::create_dir(
                    state
                        .home_dir
                        .join(&state.archive_root)
                        .join(".bbup")
                        .join("temp"),
                )?;

                // Reply with green light for push
                com.send_ok()
                    .await
                    .context("could not send greenlight for push")?;

                // Get list of changes from client
                let local_delta: structs::Delta = com
                    .get_struct()
                    .await
                    .context("could not get delta from client")?;

                // Get all files that need to be uploaded from client
                for change in &local_delta {
                    match change.change_type {
                        structs::ChangeType::Added(structs::Adding::FileType(_, _))
                        | structs::ChangeType::Edited(_) => {
                            com.send_struct(Some(change.path.clone()))
                                .await
                                .context(format!(
                                    "could not send path for file to send at path: {:?}",
                                    change.path.clone(),
                                ))?;

                            let full_path = state
                                .home_dir
                                .join(&state.archive_root)
                                .join(".bbup")
                                .join("temp")
                                .join(change.path.to_path_buf());
                            com.get_file_to(&full_path)
                                .await
                                .context(format!("could not get file at path: {full_path:?}"))?;
                        }
                        _ => {}
                    };
                }
                // send stop
                com.send_struct(None::<PathBuf>)
                    .await
                    .context("could not send empty path to signal end of file transfer")?;

                for change in &local_delta {
                    let path = state
                        .home_dir
                        .join(&state.archive_root)
                        .join(&endpoint.to_path_buf())
                        .join(&change.path.to_path_buf());
                    let from_temp_path = state
                        .home_dir
                        .join(&state.archive_root)
                        .join(".bbup")
                        .join("temp")
                        .join(&change.path.to_path_buf());

                    match change.change_type {
                        structs::ChangeType::Added(structs::Adding::Dir) => {
                            std::fs::create_dir(&path).context(format!(
                                "could not create directory to apply update\npath: {:?}",
                                path
                            ))?
                        }
                        structs::ChangeType::Added(structs::Adding::FileType(_, _))
                        | structs::ChangeType::Edited(_) => {
                            std::fs::rename(&from_temp_path, &path).context(format!(
                                "could not copy file from temp to apply update\npath: {:?}",
                                path
                            ))?;
                        }
                        structs::ChangeType::Removed(structs::Removing::Dir) => {
                            std::fs::remove_dir(&path).context(format!(
                                "could not remove directory to apply update\npath: {:?}",
                                path
                            ))?
                        }
                        structs::ChangeType::Removed(structs::Removing::FileType(_)) => {
                            std::fs::remove_file(&path).context(format!(
                                "could not remove file to apply update\npath: {:?}",
                                path
                            ))?
                        } // (structs::Action::Removed, structs::ObjectType::Dir) => {
                          //     std::fs::remove_dir(&path).context(format!(
                          //         "could not remove directory to apply update\npath: {:?}",
                          //         path
                          //     ))?
                          // }
                          // (structs::Action::Removed, _) => std::fs::remove_file(&path).context(
                          //     format!("could not remove file to apply update\npath: {:?}", path),
                          // )?,
                          // (structs::Action::Added, structs::ObjectType::Dir) => {
                          //     std::fs::create_dir(&path).context(format!(
                          //         "could not create directory to apply update\npath: {:?}",
                          //         path
                          //     ))?
                          // }
                          // (structs::Action::Edited, structs::ObjectType::Dir) => {
                          //     unreachable!("Dir cannot be edited: broken update delta")
                          // }
                          // (structs::Action::Added, _) | (structs::Action::Edited, _) => {
                          //     std::fs::rename(&from_temp_path, &path).context(format!(
                          //         "could not copy file from temp to apply update\npath: {:?}",
                          //         path
                          //     ))?;
                          // }
                    }
                }

                let local_delta: structs::Delta = local_delta
                    .into_iter()
                    .map(|change| structs::Change {
                        path: endpoint.join(&change.path),
                        ..change
                    })
                    .collect();
                let commit_id = random::random_hex(64);
                state.commit_list.push(structs::Commit {
                    commit_id: commit_id.clone(),
                    delta: local_delta,
                });
                state.save().context("could not save push update")?;

                com.send_struct(commit_id)
                    .await
                    .context("could not send commit id for the push")?;
            }
        }
    }

    Ok(())
}
