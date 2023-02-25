use crate::model::{LinkState, ProcessConfig};

use abst_fs::{self as fs, AbstPath};
use bbup_com::BbupCom;
use fs_vcs::{generate_fstree, get_actions, Action, CommitID, Delta, ExcludeList};

use anyhow::{Context, Result};

pub async fn pull(
    state: &mut LinkState,
    com: &mut BbupCom,
    link_root: &AbstPath,
    exclude_list: &ExcludeList,
    verbose: bool,
) -> Result<()> {
    // Get local delta
    if verbose {
        println!("calculating local delta...")
    }

    let new_tree = generate_fstree(link_root, exclude_list)?;
    let local_delta = state.last_known_fstree.get_delta_to(&new_tree);

    if verbose {
        if local_delta.is_empty() {
            println!("local delta: no local changes to push")
        } else {
            println!("local delta:\n{local_delta}")
        }
    }

    if verbose {
        println!("pulling from server...")
    }
    // [PULL] Send last known commit to pull updates in case of any
    com.send_struct(&state.last_known_commit)
        .await
        .context("could not send last known commit")?;

    // [PULL] Get delta from last_known_commit to server's most recent commit
    let mut update_delta: Delta = com
        .get_struct()
        .await
        .context("could not get update delta from server")?;
    let update_id: CommitID = com
        .get_struct()
        .await
        .context("could not get update id from server")?;

    // [PULL] Filter out updates that match the exclude_list
    update_delta.filter_out(exclude_list);

    if verbose {
        if update_delta.is_empty() {
            println!("pull delta: no missed change to pull")
        } else {
            println!("pull delta:\n{update_delta}")
        }
    }

    // Apply update
    // Check for conflicts or get the necessary actions
    let necessary_actions = match get_actions(&local_delta, &update_delta) {
        Ok(actions) => actions,
        Err(conflicts) => {
            println!("conflicts:\n{conflicts}");
            anyhow::bail!(
                "found conflicts between pulled update and local changes. Resolve manually"
            )
        }
    };

    // Check if it is possible to apply the update or something went wrong
    let mut updated_fstree = state.last_known_fstree.clone();
    updated_fstree.apply_delta(&update_delta)?;

    // Download files that need to be downloaded
    let mut queries = Vec::new();
    for (path, action) in &necessary_actions {
        match action {
            Action::AddFile(_, hash) | Action::EditFile(_, Some(hash)) => {
                queries.push((path.clone(), hash.clone()))
            }
            _ => {}
        }
    }
    com.send_struct(
        queries
            .iter()
            .map(|(p, _)| p.clone())
            .collect::<Vec<AbstPath>>(),
    )
    .await
    .context("could not send queries to server")?;

    let cache_path = link_root.add_last(".bbup").add_last("cache");

    for (path, hash) in queries {
        com.get_file_to_hash_check(&cache_path.append(&path), hash)
            .await
            .context(format!("could not get file at path {path}"))?;
    }

    // Apply actions
    for (path, action) in necessary_actions {
        let to_path = link_root.append(&path);
        let from_cache_path = cache_path.append(&path);
        let errmsg = |msg: &str| -> String {
            format!(
                "could not {} to apply update\npath: {}",
                msg,
                to_path.clone()
            )
        };
        match action {
            Action::AddDir => {
                fs::create_dir(&to_path).context(errmsg("create added directory"))?;
            }
            Action::AddFile(mtime, _) => {
                fs::rename_file(&from_cache_path, &to_path)
                    .context(errmsg("move added file from cache"))?;
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of added file"))?;
            }
            Action::AddSymLink(mtime, endpoint) => {
                fs::create_symlink(&to_path, endpoint).context(errmsg("create added symlink"))?;
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of added symlink"))?;
            }
            Action::EditDir(mtime) => {
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of edited directory"))?;
            }
            Action::EditFile(mtime, opth) => {
                if opth.is_some() {
                    fs::rename_file(&from_cache_path, &to_path)
                        .context(errmsg("move edited file from cache"))?;
                }
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of edited file"))?;
            }
            Action::EditSymLink(mtime, optep) => {
                if let Some(endpoint) = optep {
                    // TODO
                    // Remove and create is definitely not a pretty
                    // solution but (my) fs library is currently
                    // missing a function to overwrite an existing
                    // symlink (which basically will do this anyway
                    // under the hood because std::os::unix::fs also
                    // doesn't have a function to overwrite a symlink)
                    // so this will do for now.
                    // Same thing is going on in bbup-server/process.rs
                    fs::remove_symlink(&to_path).context(errmsg("delete edited symlink"))?;
                    fs::create_symlink(&to_path, endpoint)
                        .context(errmsg("override edited symlink"))?;
                }
                fs::set_mtime(&to_path, &mtime).context(errmsg("set mtime of edited symlink"))?;
            }
            Action::RemoveDir => {
                // Why remove_dir_all instead of just remove_dir here?
                // The reason is that the directory could contain some ignored
                // object, which wouldn't appear as a remove*** and wouldn't be
                // removed, so we have to forcefully remove it together with
                // the directory itself
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

    state.last_known_commit = update_id.clone();
    state.last_known_fstree = updated_fstree;
    state.save(link_root)?;

    Ok(())
}

pub async fn push(
    state: &mut LinkState,
    com: &mut BbupCom,
    link_root: &AbstPath,
    exclude_list: &ExcludeList,
    verbose: bool,
) -> Result<()> {
    let new_tree = generate_fstree(link_root, exclude_list)?;
    let local_delta = state.last_known_fstree.get_delta_to(&new_tree);

    com.send_struct(&state.last_known_commit).await?;
    // Await green light to procede
    com.check_ok().await?;

    com.send_struct(&local_delta).await?;

    // TODO maybe a filter-map would be a better solution here, no need
    // for queryables to be mutable. Even hiding all this inside a
    // block would be a valid solution to not make queryables mutable
    let mut queryables = Vec::new();
    for (path, action) in local_delta.to_actions()? {
        match action {
            Action::AddFile(_, _) | Action::EditFile(_, Some(_)) => queryables.push(path.clone()),
            _ => {}
        }
    }
    let queries: Vec<AbstPath> = com
        .get_struct()
        .await
        .context("could not recieve queries")?;

    for path in &queries {
        match queryables.iter().find(|p| p == &path) {
            Some(_) => {}
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

    for path in queries {
        com.send_file_from(&link_root.append(&path))
            .await
            .context(format!("could not send file at path {path}"))?;
    }

    state.last_known_commit = com.get_struct().await?;
    state.last_known_fstree = new_tree.clone();
    state.save(link_root)?;

    Ok(())
}
