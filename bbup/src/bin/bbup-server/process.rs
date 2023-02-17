use super::{hash_to_path, Archive, ArchiveEndpoint};

use abst_fs::{self as fs, AbstPath};
use fs_vcs::{Action, CommitID, Delta};
use hasher::Hash;

use bbup_com::{BbupCom, JobType};

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use tokio::{net::TcpStream, sync::Mutex};

async fn pull(endpoint_root: &AbstPath, state: &ArchiveEndpoint, com: &mut BbupCom) -> Result<()> {
    let last_known_commit: CommitID = com.get_struct().await.context("could not get lkc")?;

    // calculate update for client
    // TODO maybe this should panic because it means a broken server state
    let delta = state
        .get_update_delta(last_known_commit)
        .context("could not get update delta")?;
    let id = state.most_recent_commit().commit_id.clone();

    // send update delta to client for pull
    com.send_struct(delta.clone())
        .await
        .context("could not send update delta")?;
    com.send_struct(id)
        .await
        .context("could not send update id")?;

    // TODO maybe a filter-map would be a better solution here, no need for
    // queryables to be mutable. Even hiding all this inside a block would be a
    // valid solution to not make queryables mutable
    let mut queryables = Vec::new();
    for (path, action) in delta.to_actions()? {
        match action {
            Action::AddFile(_, hash) | Action::EditFile(_, Some(hash)) => {
                queryables.push((path.clone(), hash.clone()));
            }
            _ => {}
        }
    }
    let queries: Vec<AbstPath> = com
        .get_struct()
        .await
        .context("could not recieve queries")?;

    let mut query_hashes = Vec::new();
    for path in queries {
        match queryables.iter().find(|p| p.0 == path) {
            Some((_, hash)) => query_hashes.push(hash),
            None => {
                com.send_error(1, "quered file at path not allowed")
                    .await
                    .context(
                        "could not propagate error to client [quered file at path not allowed]",
                    )?;
                anyhow::bail!("Client quered file at path not allowed [{path}]")
            }
        }
    }
    for hash in query_hashes {
        let hash_path = hash_to_path(hash.clone());
        com.send_file_from(&endpoint_root.add_last("objects").append(&hash_path))
            .await
            .context(format!("could not send file at path {hash_path}"))?
    }

    Ok(())
}

async fn push(
    endpoint_root: &AbstPath,
    state: &mut ArchiveEndpoint,
    com: &mut BbupCom,
) -> Result<()> {
    let cache_path = &endpoint_root.add_last("cache");
    let objects_path = &endpoint_root.add_last("objects");
    fs::make_clean_dir(cache_path)?;

    // Reply with green light for push
    com.send_ok()
        .await
        .context("could not send greenlight for push")?;

    // Get list of changes from client
    let local_delta: Delta = com
        .get_struct()
        .await
        .context("could not get delta from client")?;

    // TODO if fail, send error message to the server
    // TODO checking if the delta is applicable and then applying it at the end
    // of the function is a bit resource wasteful as we're practically applying
    // the delta twice. I'm using this approach because this way it's more
    // clear that here we're just checking if this delta is a valid one, but
    // maybe it's too much wasteful
    state.is_delta_applicable(&local_delta)?;

    // Get all files that need to be uploaded from client
    // We pass through a hashmap to avoid querying multiple files with the same
    // hash (hence with the same content, which we only need once to save in
    // the "objects" directory)
    let mut queries = HashMap::new();
    for (path, action) in &local_delta.to_actions()? {
        match action {
            Action::AddFile(_, hash) | Action::EditFile(_, Some(hash)) => {
                queries.insert(hash.clone(), path.clone());
            }
            _ => {}
        }
    }
    let (query_hashes, query_paths): (Vec<Hash>, Vec<AbstPath>) = queries.into_iter().unzip();
    com.send_struct(query_paths)
        .await
        .context("could not send queries to client")?;

    for hash in &query_hashes {
        let hash_path = hash_to_path(hash.clone());
        com.get_file_to_hash_check(&cache_path.append(&hash_path), hash.clone())
            .await
            .context(format!("could not get file at path {hash_path}"))?;
    }

    for hash in query_hashes {
        let hash_path = hash_to_path(hash.clone());
        let to_path = objects_path.append(&hash_path);
        let from_cache_path = cache_path.append(&hash_path);

        fs::rename_file(&from_cache_path, &to_path).context(format!(
            "could not move object from temp to destination at path {hash_path}",
        ))?;
    }

    let commit_id = state
        .commit_delta(&local_delta)
        .context("could not update state")?;
    state
        .save(endpoint_root)
        .context("could not save push update")?;

    com.send_struct(commit_id)
        .await
        .context("could not send commit id for the push")?;
    Ok(())
}

pub async fn process_connection(
    archive_root: &AbstPath,
    socket: TcpStream,
    state: Arc<Mutex<Archive>>,
    progress: bool,
) -> Result<()> {
    let mut com = BbupCom::from(socket, progress);

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

        let endpoint: String = com
            .get_struct()
            .await
            .context("could not get backup endpoint")?;
        let endpoint_state = state.get_mut(&endpoint).context("endpoint doesn't exist")?;

        com.send_ok()
            .await
            .context("could not send greenlight for validity of endpoint")?;

        let endpoint_root = ArchiveEndpoint::endpoint_root(archive_root, &endpoint);

        loop {
            let jt: JobType = com.get_struct().await.context("could not get job type")?;
            match jt {
                JobType::Quit => {
                    com.send_ok().await?;
                    break;
                }
                JobType::Pull => {
                    pull(&endpoint_root, endpoint_state, &mut com).await?;
                }
                JobType::Push => {
                    push(&endpoint_root, endpoint_state, &mut com).await?;
                }
            }
        }

        Ok(())
    };

    match conversation_result {
        Ok(()) => Ok(()),
        Err(error) => {
            if let Err(err) = com.send_error(1, "error propagated from server").await {
                println!("Could not propagate error to client, because {:#?}", err)
            }
            Err(error)
        }
    }
}
