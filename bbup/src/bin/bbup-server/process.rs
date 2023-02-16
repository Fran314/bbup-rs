use super::{hash_to_path, Archive, ArchiveEndpoint};

use abst_fs::{self as fs, AbstPath};
use fs_vcs::{Action, CommitID, Delta};

use bbup_com::{BbupCom, JobType};

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::{net::TcpStream, sync::Mutex};

async fn pull(
    endpoint_root: &AbstPath,
    state: &ArchiveEndpoint,
    com: &mut BbupCom,
    // endpoint: &AbstPath,
) -> Result<()> {
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

    let actions = delta.to_actions()?;
    // TODO maybe a filter-map would be a better solution here, no need for
    // queryables to be mutable. Even hiding all this inside a block would be a
    // valid solution to not make queryables mutable
    let mut queryables = Vec::new();
    for (path, action) in actions {
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

    // send all files requested by client
    // com.supply_mapped_files(queryables2, &config.archive_root.append(endpoint))
    //     .await
    //     .context("could not supply files to download update")?;

    Ok(())
}

async fn push(
    endpoint_root: &AbstPath,
    state: &mut ArchiveEndpoint,
    com: &mut BbupCom,
    // endpoint: &AbstPath,
) -> Result<()> {
    // TODO .bbup isn't really necessary here, but has to be modified
    // consistently in other places
    fs::make_clean_dir(&endpoint_root.add_last(".bbup").add_last("temp"))?;

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
    // let rebased_delta = local_delta.rebase_from_tree_at_endpoint(&state.archive_tree, endpoint)?;
    // let mut updated_archive_tree = state.archive_tree.clone();
    // updated_archive_tree.apply_delta(&rebased_delta)?;

    // Get all files that need to be uploaded from client
    let actions = local_delta.to_actions()?;
    let mut query_paths = Vec::new();
    let mut query_hashes = Vec::new();
    for (path, action) in &actions {
        match action {
            Action::AddFile(_, hash) | Action::EditFile(_, Some(hash)) => {
                query_paths.push(path.clone());
                query_hashes.push(hash.clone());
            }
            _ => {}
        }
    }
    com.send_struct(query_paths)
        .await
        .context("could not send queries to client")?;

    for hash in &query_hashes {
        let hash_path = hash_to_path(hash.clone());
        com.get_file_to_hash_check(
            &endpoint_root
                .add_last(".bbup")
                .add_last("temp")
                .append(&hash_path),
            hash.clone(),
        )
        .await
        .context(format!("could not get file at path {hash_path}"))?;
    }

    for hash in query_hashes {
        let hash_path = hash_to_path(hash.clone());
        let to_path = endpoint_root.add_last("objects").append(&hash_path);
        let from_temp_path = endpoint_root
            .add_last(".bbup")
            .add_last("temp")
            .append(&hash_path);

        fs::rename_file(&from_temp_path, &to_path).context(format!(
            "could not move object from temp to destination at path {hash_path}",
        ))?;
    }

    // com.query_files(
    //     queries,
    //     &config.archive_root.add_last(".bbup").add_last("temp"),
    // )
    // .await
    // .context("could not query files to apply push")?;

    // for (path, action) in &actions {
    //     let to_path = config.archive_root.append(endpoint).append(path);
    //     let from_temp_path = config
    //         .archive_root
    //         .add_last(".bbup")
    //         .add_last("temp")
    //         .append(path);
    //
    //     let errmsg = |msg: &str| -> String {
    //         format!(
    //             "could not {} to apply new commit\npath: {}",
    //             msg,
    //             to_path.clone()
    //         )
    //     };
    //     match action {
    //         Action::AddDir => {
    //             fs::create_dir(&to_path).context(errmsg("create added directory"))?;
    //         }
    //         Action::AddFile(mtime, _) => {
    //             fs::rename_file(&from_temp_path, &to_path)
    //                 .context(errmsg("move added file from temp"))?;
    //             fs::set_mtime(&to_path, mtime).context(errmsg("set mtime of added file"))?;
    //         }
    //         Action::AddSymLink(mtime, endpoint) => {
    //             fs::create_symlink(&to_path, endpoint.clone())
    //                 .context(errmsg("create added symlink"))?;
    //             fs::set_mtime(&to_path, mtime).context(errmsg("set mtime of added symlink"))?;
    //         }
    //         Action::EditDir(mtime) => {
    //             fs::set_mtime(&to_path, mtime).context(errmsg("set mtime of edited directory"))?;
    //         }
    //         Action::EditFile(mtime, opth) => {
    //             if opth.is_some() {
    //                 fs::rename_file(&from_temp_path, &to_path)
    //                     .context(errmsg("move edited file from temp"))?;
    //             }
    //             fs::set_mtime(&to_path, mtime).context(errmsg("set mtime of edited file"))?;
    //         }
    //         Action::EditSymLink(mtime, optep) => {
    //             if let Some(endpoint) = optep {
    //                 // TODO
    //                 // Remove and create is definitely not a pretty solution
    //                 // but (my) fs library is currently missing a function to
    //                 // overwrite an existing symlink (which basically will do
    //                 // this anyway under the hood because std::os::unix::fs
    //                 // also doesn't have a function to overwrite a symlink) so
    //                 // this will do for now.
    //                 // Same thing is going on in bbup-server/process.rs
    //                 fs::remove_symlink(&to_path).context(errmsg("delete edited symlink"))?;
    //                 fs::create_symlink(&to_path, endpoint.clone())
    //                     .context(errmsg("override edited symlink"))?;
    //             }
    //             fs::set_mtime(&to_path, mtime).context(errmsg("set mtime of edited symlink"))?;
    //         }
    //         Action::RemoveDir => {
    //             // Why remove_dir_all instead of just remove_dir here?
    //             // One would think that, because the delta.flatten() flattens a
    //             //	removed directory by recursively adding a removefile/
    //             //	removesymlink/removedir for all the nested childs, once we get
    //             //	at a removedir we can be sure that the directory is actually
    //             //	empty.
    //             // This is not true because the directory could contain some
    //             //	ignored object, which wouldn't appear as a remove*** and
    //             //	wouldn't be removed, so we have to forcefully remove it
    //             //	together with the directory itself
    //             fs::remove_dir_all(&to_path).context(errmsg("remove deleted dir"))?;
    //         }
    //         Action::RemoveFile => {
    //             fs::remove_file(&to_path).context(errmsg("remove deleted file"))?;
    //         }
    //         Action::RemoveSymLink => {
    //             fs::remove_symlink(&to_path).context(errmsg("remove deleted symlink"))?;
    //         }
    //     }
    // }

    let commit_id = state
        .commit_delta(&local_delta)
        .context("could not update state")?;
    // let commit_id = Commit::gen_valid_id();
    // state.commit_list.push(Commit {
    //     commit_id: commit_id.clone(),
    //     delta: rebased_delta,
    // });
    // state.archive_tree = updated_archive_tree;
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

        let endpoint_root = archive_root.append(&AbstPath::from(endpoint));

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
