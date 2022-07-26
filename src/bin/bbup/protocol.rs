use bbup_rust::fstree::Change;
use std::path::PathBuf;

use crate::{ProcessConfig, ProcessState};

use bbup_rust::com::{BbupCom, Querable};
use bbup_rust::hash::Hash;
use bbup_rust::model::Commit;
use bbup_rust::{fs, fstree};

use anyhow::{Context, Result};

pub fn get_local_delta(config: &ProcessConfig, state: &mut ProcessState) -> Result<()> {
    if config.flags.verbose {
        println!("calculating local delta...")
    }

    let new_tree = fstree::generate_fstree(&config.link_root, &config.exclude_list)?;
    let local_delta = fstree::get_delta(&state.last_known_fstree, &new_tree);

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
    let mut update: Commit = com
        .get_struct()
        .await
        .context("could not get update-delta from server")?;

    // [PULL] Filter out updates that match the exclude_list
    update.delta.filter_out(&config.exclude_list);

    if config.flags.verbose {
        if update.delta.is_empty() {
            println!("pull delta: no missed change to pull")
        } else {
            println!("pull delta:\n{}", update.delta)
        }
    }

    state.update = Some(update);

    Ok(())
}

pub async fn check_for_conflicts(state: &mut ProcessState) -> Result<()> {
    match (&state.local_delta, &state.update) {
        (
            Some(local_delta),
            Some(Commit {
                commit_id: _,
                delta: update_delta,
            }),
        ) => {
            let conflicts = fstree::check_for_conflicts(local_delta, update_delta);
            if let Some(conflicts) = conflicts {
                println!("conflicts:\n{}", conflicts);

                anyhow::bail!(
                    "found conflicts between pulled update and local changes. Resolve manually"
                )
            }
            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for check-for-conflicts but is missing\nstate.local_delta: {}\nstate.update: {}",
				state.local_delta.is_some(),
				state.update.is_some(),
			)
        }
    }
}

pub async fn download_update(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom,
) -> Result<()> {
    match &state.update {
        Some(Commit {
            commit_id: _,
            delta: update_delta,
        }) => {
            // Get all files that need to be downloaded from server
            let queries: Vec<(Querable, PathBuf, Hash)> = update_delta
                .flatten()
                .into_iter()
                .flat_map(|(path, change)| match change {
                    Change::AddFile(_, hash) | Change::EditFile(_, Some(hash)) => {
                        vec![(Querable::File, path, hash)]
                    }
                    Change::AddSymLink(hash) | Change::EditSymLink(hash) => {
                        vec![(Querable::SymLink, path, hash)]
                    }
                    _ => vec![],
                })
                .collect();

            com.query_files(queries, &config.link_root.join(".bbup").join("temp"))
                .await
                .context("could not query files and symlinks to apply update")?;

            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for download-update but is missing\nstate.update: {}",
				state.update.is_some(),
			)
        }
    }
}

pub async fn apply_update(config: &ProcessConfig, state: &mut ProcessState) -> Result<()> {
    match &state.update {
        Some(Commit { commit_id, delta }) => {
            let updated_fstree = state.last_known_fstree.try_apply_delta(delta)?;

            for (path, change) in delta.flatten() {
                let to_path = config.link_root.join(&path);
                let from_temp_path = config.link_root.join(".bbup").join("temp").join(&path);
                let errmsg = |msg: &str| -> String {
                    format!(
                        "could not {} to apply update\npath: {:?}",
                        msg,
                        to_path.clone()
                    )
                };
                match change {
                    Change::AddDir(metadata) => {
                        fs::create_dir(&to_path).context(errmsg("create added directory"))?;
                        fs::set_metadata(&to_path, &metadata)
                            .context(errmsg("set metadata of added directory"))?;
                    }
                    Change::AddFile(metadata, _) => {
                        fs::rename_file(from_temp_path, &to_path)
                            .context(errmsg("move added file from temp"))?;
                        fs::set_metadata(&to_path, &metadata)
                            .context(errmsg("set metadata of added file"))?;
                    }
                    Change::AddSymLink(_) => {
                        fs::rename_symlink(from_temp_path, &to_path)
                            .context(errmsg("move added symlink from temp"))?;
                    }
                    Change::EditDir(metadata) => {
                        fs::set_metadata(&to_path, &metadata)
                            .context(errmsg("set metadata of edited directory"))?;
                    }
                    Change::EditFile(optm, opth) => {
                        if let Some(_) = opth {
                            fs::rename_file(from_temp_path, &to_path)
                                .context(errmsg("move edited file from temp"))?;
                        }
                        if let Some(metadata) = optm {
                            fs::set_metadata(&to_path, &metadata)
                                .context(errmsg("set metadata of edited file"))?;
                        }
                    }
                    Change::EditSymLink(_) => {
                        fs::rename_symlink(from_temp_path, &to_path)
                            .context(errmsg("move edited symlink from temp"))?;
                    }
                    Change::RemoveDir => {
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
                    Change::RemoveFile => {
                        fs::remove_file(&to_path).context(errmsg("remove deleted file"))?;
                    }
                    Change::RemoveSymLink => {
                        fs::remove_symlink(&to_path).context(errmsg("remove deleted symlink"))?;
                    }
                }
            }
            state.last_known_commit = commit_id.clone();
            state.last_known_fstree = updated_fstree;
            state.save(&config.link_root)?;

            let new_tree = fstree::generate_fstree(&config.link_root, &config.exclude_list)?;
            let local_delta = fstree::get_delta(&state.last_known_fstree, &new_tree);

            state.new_tree = Some(new_tree);
            state.local_delta = Some(local_delta);

            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for apply-update but is missing\nstate.update: {}",
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

            let querable: Vec<PathBuf> = local_delta
                .flatten()
                .into_iter()
                .filter(|(_, change)| match change {
                    Change::AddFile(_, _)
                    | Change::AddSymLink(_)
                    | Change::EditFile(_, _)
                    | Change::EditSymLink(_) => true,
                    _ => false,
                })
                .map(|(path, _)| path)
                .collect();

            com.supply_files(&querable, &config.link_root)
                .await
                .context("could not supply files and symlinks to upload push")?;

            state.last_known_commit = com.get_struct().await?;
            state.last_known_fstree = new_tree.clone();
            state.save(&&config.link_root)?;

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
