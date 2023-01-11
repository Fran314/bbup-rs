use super::{ArchiveConfig, ArchiveState};

use abst_fs::{self as fs, AbstPath};
use fs_vcs::{Action, Commit, Delta};

use bbup::com::{BbupCom, JobType, Queryable};

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::{net::TcpStream, sync::Mutex};

async fn pull(
    config: &ArchiveConfig,
    state: &ArchiveState,
    com: &mut BbupCom,
    endpoint: &AbstPath,
) -> Result<()> {
    let last_known_commit: String = com.get_struct().await.context("could not get lkc")?;

    // calculate update for client
    // TODO maybe this should panic because it means a broken server state
    let delta = state
        .commit_list
        .get_update_delta(endpoint, last_known_commit)
        .context("could not get update delta")?;
    let id = state.commit_list.most_recent_commit().commit_id.clone();

    // send update delta to client for pull
    com.send_struct(delta.clone())
        .await
        .context("could not send update delta")?;
    com.send_struct(id)
        .await
        .context("could not send update id")?;

    let mut queryables = Vec::new();
    for (path, action) in &delta.to_actions() {
        match action {
            Action::AddFile(_, _)
            | Action::EditFile(_, Some(_))
            | Action::AddSymLink(_, _)
            | Action::EditSymLink(_, Some(_)) => queryables.push(path.clone()),

            _ => {}
        }
    }

    // send all files requested by client
    com.supply_files(&queryables, &config.archive_root.append(endpoint))
        .await
        .context("could not supply files to download update")?;

    Ok(())
}

async fn push(
    config: &ArchiveConfig,
    state: &mut ArchiveState,
    com: &mut BbupCom,
    endpoint: &AbstPath,
) -> Result<()> {
    fs::make_clean_dir(&config.archive_root.add_last(".bbup").add_last("temp"))?;

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
    let mut queries = Vec::new();
    for (path, action) in &local_delta.to_actions() {
        match action {
            Action::AddFile(_, hash) | Action::EditFile(_, Some(hash)) => {
                queries.push((Queryable::File, path.clone(), hash.clone()))
            }

            Action::AddSymLink(_, hash) | Action::EditSymLink(_, Some(hash)) => {
                queries.push((Queryable::SymLink, path.clone(), hash.clone()))
            }

            _ => {}
        }
    }
    com.query_files(
        queries,
        &config.archive_root.add_last(".bbup").add_last("temp"),
    )
    .await
    .context("could not query files to apply push")?;

    // TODO if fail, send error message to the server
    let rebased_delta = local_delta.rebase_from_tree_at_endpoint(&state.archive_tree, &endpoint)?;
    let mut updated_archive_tree = state.archive_tree.clone();
    updated_archive_tree.apply_delta(&rebased_delta)?;
    // updated_archive_tree.apply_delta_at_endpoint(&local_delta, endpoint.clone())?;

    for (path, action) in local_delta.to_actions() {
        let to_path = config.archive_root.append(endpoint).append(&path);
        let from_temp_path = config
            .archive_root
            .add_last(".bbup")
            .add_last("temp")
            .append(&path);

        let errmsg = |msg: &str| -> String {
            format!(
                "could not {} to apply new commit\npath: {}",
                msg,
                to_path.clone()
            )
        };
        match action {
            Action::AddDir => {
                fs::create_dir(&to_path).context(errmsg("create added directory"))?;
            }
            Action::AddFile(mtime, _) => {
                fs::rename_file(&from_temp_path, &to_path)
                    .context(errmsg("move added file from temp"))?;
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of added file"))?;
            }
            Action::AddSymLink(mtime, _) => {
                fs::rename_symlink(&from_temp_path, &to_path)
                    .context(errmsg("move added symlink from temp"))?;
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of added symlink"))?;
            }
            Action::EditDir(mtime) => {
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of edited directory"))?;
            }
            Action::EditFile(optm, opth) => {
                if opth.is_some() {
                    fs::rename_file(&from_temp_path, &to_path)
                        .context(errmsg("move edited file from temp"))?;
                }
                if let Some(mtime) = optm {
                    fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of edited file"))?;
                }
            }
            Action::EditSymLink(optm, opth) => {
                if opth.is_some() {
                    fs::rename_symlink(&from_temp_path, &to_path)
                        .context(errmsg("move edited symlink from temp"))?;
                }
                if let Some(mtime) = optm {
                    fs::set_mtime(&to_path, &mtime)
                        .context(errmsg("set mtime of edited symlink"))?;
                }
            }
            Action::RemoveDir => {
                // Why remove_dir_all instead of just remove_dir here?
                // One would think that, because the delta.flatten() flattens a
                //	removed directory by recursively adding a removefile/
                //	removesymlink/removedir for all the nested childs, once we get
                //	at a removedir we can be sure that the directory is actually
                //	empty.
                // This is not true because the directory could contain some
                //	ignored object, which wouldn't appear as a remove*** and
                //	wouldn't be removed, so we have to forcefully remove it
                //	together with the directory itself
                fs::remove_dir_all(&to_path).context(errmsg("remove deleted dir"))?;
            }
            Action::RemoveFile => {
                fs::remove_file(&to_path).context(errmsg("remove deleted file"))?;
            }
            Action::RemoveSymLink => {
                fs::remove_symlink(&to_path).context(errmsg("remove deleted symlink"))?;
            }
        }
    }

    let commit_id = Commit::gen_valid_id();
    state.commit_list.push(Commit {
        commit_id: commit_id.clone(),
        // endpoint: endpoint.clone(),
        delta: rebased_delta,
    });
    state.archive_tree = updated_archive_tree;
    state
        .save(&config.archive_root)
        .context("could not save push update")?;

    com.send_struct(commit_id)
        .await
        .context("could not send commit id for the push")?;
    Ok(())
}

pub async fn process_connection(
    config: ArchiveConfig,
    socket: TcpStream,
    state: Arc<Mutex<ArchiveState>>,
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

        let endpoint: AbstPath = com
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
                    pull(&config, &state, &mut com, &endpoint).await?;
                }
                JobType::Push => {
                    push(&config, &mut state, &mut com, &endpoint).await?;
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
