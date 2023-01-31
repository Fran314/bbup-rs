use abst_fs::AbstPath;
use fs_vcs::{generate_fstree, get_actions, get_delta, Action, Delta};

use super::{ProcessConfig, ProcessState};

use abst_fs as fs;
use bbup_com::BbupCom;

use anyhow::{Context, Result};

pub fn get_local_delta(config: &ProcessConfig, state: &mut ProcessState) -> Result<()> {
    if config.flags.verbose {
        println!("calculating local delta...")
    }

    let new_tree = generate_fstree(&config.link_root, &config.exclude_list)?;
    let local_delta = get_delta(&state.last_known_fstree, &new_tree);

    if config.flags.verbose {
        if local_delta.is_empty() {
            println!("local delta: no local changes to push")
        } else {
            println!("local delta:\n{}", local_delta)
        }
    }

    state.new_tree = Some(new_tree);
    state.local_delta = Some(local_delta);
    Ok(())
}

pub async fn pull_update_delta(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom,
) -> Result<()> {
    if config.flags.verbose {
        println!("pulling from server...")
    }
    // [PULL] Send last known commit to pull updates in case of any
    com.send_struct(&state.last_known_commit)
        .await
        .context("could not send last known commit")?;

    // [PULL] Get delta from last_known_commit to server's most recent commit
    let mut delta: Delta = com
        .get_struct()
        .await
        .context("could not get update delta from server")?;
    let id: String = com
        .get_struct()
        .await
        .context("could not get update id from server")?;

    // [PULL] Filter out updates that match the exclude_list
    delta.filter_out(&config.exclude_list);

    if config.flags.verbose {
        if delta.is_empty() {
            println!("pull delta: no missed change to pull")
        } else {
            println!("pull delta:\n{}", delta)
        }
    }

    state.update = Some((id, delta));

    Ok(())
}

pub async fn apply_update_or_get_conflicts(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom,
) -> Result<()> {
    match (&state.local_delta, &state.update) {
        (Some(local_delta), Some((update_id, update_delta))) => {
            // Check for conflicts or get the necessary actions
            let necessary_actions = match get_actions(local_delta, update_delta) {
                Ok(actions) => actions,
                Err(conflicts) => {
                    println!("conflicts:\n{}", conflicts);
                    anyhow::bail!(
                        "found conflicts between pulled update and local changes. Resolve manually"
                    )
                }
            };

            // Check if it is possible to apply the update or something went wrong
            let mut updated_fstree = state.last_known_fstree.clone();
            updated_fstree.apply_delta(update_delta)?;

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

            for (path, hash) in queries {
                com.get_file_to_hash_check(
                    &config
                        .link_root
                        .add_last(".bbup")
                        .add_last("temp")
                        .append(&path),
                    hash,
                )
                .await
                .context(format!("could not get file at path {path}"))?;
            }

            // com.query_files(
            //     queries,
            //     &config.link_root.add_last(".bbup").add_last("temp"),
            // )
            // .await
            // .context("could not query files and symlinks to apply update")?;

            // Apply actions
            for (path, action) in necessary_actions {
                let to_path = config.link_root.append(&path);
                let from_temp_path = config
                    .link_root
                    .add_last(".bbup")
                    .add_last("temp")
                    .append(&path);
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
                        fs::rename_file(&from_temp_path, &to_path)
                            .context(errmsg("move added file from temp"))?;
                        fs::set_mtime(&to_path, &mtime)
                            .context(errmsg("set mtime of added file"))?;
                    }
                    Action::AddSymLink(mtime, endpoint) => {
                        fs::create_symlink(&to_path, endpoint)
                            .context(errmsg("create added symlink"))?;
                        fs::set_mtime(&to_path, &mtime)
                            .context(errmsg("set mtime of added symlink"))?;
                    }
                    Action::EditDir(mtime) => {
                        fs::set_mtime(&to_path, &mtime)
                            .context(errmsg("set mtime of edited directory"))?;
                    }
                    Action::EditFile(mtime, opth) => {
                        if opth.is_some() {
                            fs::rename_file(&from_temp_path, &to_path)
                                .context(errmsg("move edited file from temp"))?;
                        }
                        fs::set_mtime(&to_path, &mtime)
                            .context(errmsg("set mtime of edited file"))?;
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
                            fs::remove_symlink(&to_path)
                                .context(errmsg("delete edited symlink"))?;
                            fs::create_symlink(&to_path, endpoint)
                                .context(errmsg("override edited symlink"))?;
                        }
                        fs::set_mtime(&to_path, &mtime)
                            .context(errmsg("set mtime of edited symlink"))?;
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
            state.last_known_commit = update_id.clone();
            state.last_known_fstree = updated_fstree;
            state.save(&config.link_root)?;

            let new_tree = generate_fstree(&config.link_root, &config.exclude_list)?;
            let local_delta = get_delta(&state.last_known_fstree, &new_tree);

            state.new_tree = Some(new_tree);
            state.local_delta = Some(local_delta);

            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for applying update but is missing\nstate.local_delta: {}\nstate.update: {}",
				state.local_delta.is_some(),
				state.update.is_some(),
			)
        }
    }
}

pub async fn upload_changes(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom,
) -> Result<()> {
    match (&state.local_delta, &state.new_tree) {
        (Some(local_delta), Some(new_tree)) => {
            // Await green light to procede
            com.check_ok().await?;

            com.send_struct(local_delta).await?;

            let actions = local_delta.to_actions()?;
            // TODO maybe a filter-map would be a better solution here, no need
            // for queryables to be mutable. Even hiding all this inside a
            // block would be a valid solution to not make queryables mutable
            let mut queryables = Vec::new();
            for (path, action) in actions {
                match action {
                    Action::AddFile(_, _) | Action::EditFile(_, Some(_)) => {
                        queryables.push(path.clone())
                    }
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
                com.send_file_from(&config.link_root.append(&path))
                    .await
                    .context(format!("could not send file at path {path}"))?;
            }

            // com.supply_files(queryables, &config.link_root)
            //     .await
            //     .context("could not supply files and symlinks to upload push")?;

            state.last_known_commit = com.get_struct().await?;
            state.last_known_fstree = new_tree.clone();
            state.save(&config.link_root)?;

            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for upload-changes but is missing\nstate.local_delta: {}\nstate.new_tree: {}",
				state.local_delta.is_some(),
				state.new_tree.is_some(),
			)
        }
    }
}
