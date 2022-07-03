use crate::{CommmitListExt, ServerState};

use bbup_rust::{
    com::BbupCom,
    path::AbstractPath,
    random,
    structs::{Adding, Change, ChangeType, Commit, Delta, JobType, Removing},
};

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::{net::TcpStream, sync::Mutex};

async fn pull<T, R>(
    com: &mut BbupCom<T, R>,
    state: &ServerState,
    endpoint: &AbstractPath,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    let last_known_commit: String = com.get_struct().await.context("could not get lkc")?;

    // calculate update for client
    let delta = state
        .commit_list
        .get_update_delta(&endpoint, last_known_commit);

    // send update delta to client for pull
    com.send_struct(Commit {
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
            .archive_root
            .join(&endpoint.to_path_buf())
            .join(&path.to_path_buf());

        com.send_file_from(&full_path)
            .await
            .context(format!("could not send file at path: {full_path:?}"))?;
    }

    Ok(())
}

async fn push<T, R>(
    com: &mut BbupCom<T, R>,
    state: &mut ServerState,
    endpoint: &AbstractPath,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    std::fs::create_dir_all(state.archive_root.join(".bbup").join("temp"))?;
    std::fs::remove_dir_all(state.archive_root.join(".bbup").join("temp"))?;
    std::fs::create_dir(state.archive_root.join(".bbup").join("temp"))?;

    // Reply with green light for push
    com.send_ok()
        .await
        .context("could not send greenlight for push")?;

    // Get list of changes from client
    let local_delta: Delta = com
        .get_struct()
        .await
        .context("could not get delta from client")?;

    // Get all files that need to be uploaded from client
    for change in &local_delta {
        match change.change_type {
            ChangeType::Added(Adding::FileType(_, _)) | ChangeType::Edited(_) => {
                com.send_struct(Some(change.path.clone()))
                    .await
                    .context(format!(
                        "could not send path for file to send at path: {:?}",
                        change.path.clone(),
                    ))?;

                let full_path = state
                    .archive_root
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

    // TODO if fail, send error message to the server
    let updated_archive_tree = state.archive_tree.try_apply_delta(&local_delta)?;

    for change in &local_delta {
        let path = state
            .archive_root
            .join(&endpoint.to_path_buf())
            .join(&change.path.to_path_buf());
        let from_temp_path = state
            .archive_root
            .join(".bbup")
            .join("temp")
            .join(&change.path.to_path_buf());

        match change.change_type {
            ChangeType::Added(Adding::Dir) => std::fs::create_dir(&path).context(format!(
                "could not create directory to apply update\npath: {:?}",
                path
            ))?,
            ChangeType::Added(Adding::FileType(_, _)) | ChangeType::Edited(_) => {
                std::fs::rename(&from_temp_path, &path).context(format!(
                    "could not copy file from temp to apply update\npath: {:?}",
                    path
                ))?;
            }
            ChangeType::Removed(Removing::Dir) => std::fs::remove_dir(&path).context(format!(
                "could not remove directory to apply update\npath: {:?}",
                path
            ))?,
            ChangeType::Removed(Removing::FileType(_)) => std::fs::remove_file(&path).context(
                format!("could not remove file to apply update\npath: {:?}", path),
            )?,
        }
    }

    let local_delta: Delta = local_delta
        .into_iter()
        .map(|change| Change {
            path: endpoint.join(&change.path),
            ..change
        })
        .collect();
    let commit_id = random::random_hex(64);
    state.commit_list.push(Commit {
        commit_id: commit_id.clone(),
        delta: local_delta.clone(),
    });
    state.archive_tree = updated_archive_tree;
    state.save().context("could not save push update")?;

    com.send_struct(commit_id)
        .await
        .context("could not send commit id for the push")?;
    Ok(())
}

pub async fn process_connection(
    socket: TcpStream,
    state: Arc<Mutex<ServerState>>,
    progress: bool,
) -> Result<()> {
    let mut com = BbupCom::from_split(socket.into_split(), progress);

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

    let conversation_result: Result<()> = {
        // Reply with green light to conversation, send OK
        com.send_ok()
            .await
            .context("could not send greenlight for conversation")?;

        let endpoint: AbstractPath = com
            .get_struct()
            .await
            .context("could not get backup endpoint")?;

        loop {
            let jt: JobType = com.get_struct().await.context("could not get job type")?;
            match jt {
                JobType::Quit => {
                    com.send_ok().await?;
                    break;
                }
                JobType::Pull => {
                    pull(&mut com, &state, &endpoint).await?;
                }
                JobType::Push => {
                    push(&mut com, &mut state, &endpoint).await?;
                }
            }
        }

        Ok(())
    };

    match conversation_result {
        Ok(()) => Ok(()),
        Err(error) => {
            match com.send_error(1, "error propagated from server").await {
                Err(err) => {
                    println!("Could not propagate error to client, because {:#?}", err)
                }
                _ => {}
            }
            Err(error)
        }
    }
}
