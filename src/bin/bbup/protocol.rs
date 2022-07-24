use bbup_rust::fstree::Change;
use std::path::PathBuf;

use crate::{ProcessConfig, ProcessState};

use bbup_rust::com::BbupCom;
use bbup_rust::model::{Commit, Query};
use bbup_rust::{fs, fstree};

use anyhow::{Context, Result};

pub fn get_local_delta(config: &ProcessConfig, state: &mut ProcessState) -> Result<()> {
    if config.flags.verbose {
        println!("calculating local delta...")
    }

    state.last_known_commit = Some(fs::load(
        &config
            .link_root
            .join(".bbup")
            .join("last-known-commit.json"),
    )?);

    let old_tree = fs::load(&config.link_root.join(".bbup").join("old-hash-tree.json"))?;
    let new_tree = fstree::generate_fstree(&config.link_root, &config.exclude_list)?;
    let local_delta = fstree::get_delta(&old_tree, &new_tree);

    if config.flags.verbose {
        if local_delta.is_empty() {
            println!("local delta: no local changes to push")
        } else {
            println!("local delta:\n{}", local_delta)
        }
    }

    state.old_tree = Some(old_tree);
    state.new_tree = Some(new_tree);
    state.local_delta = Some(local_delta);
    Ok(())
}

pub async fn pull_update_delta(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom,
) -> Result<()> {
    match &state.last_known_commit {
        Some(lkc) => {
            if config.flags.verbose {
                println!("pulling from server...")
            }
            // [PULL] Send last known commit to pull updates in case of any
            com.send_struct(lkc)
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
        _ => {
            anyhow::bail!(
                "Some part of the state was required for pull-update-delta but is missing\nstate.last_known_commit: {}",
                &state.last_known_commit.is_some()
            )
        }
    }
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
            for (path, change) in update_delta.flatten() {
                let full_path = &config
                    .link_root
                    .join(".bbup")
                    .join("temp")
                    .join(path.clone());
                match change {
                    Change::AddFile(_, hash) | Change::EditFile(_, Some(hash)) => {
                        // TODO somehow use the hashes to check if the data arrived correctly
                        com.send_struct(Query::FileAt(path.clone())).await?;
                        com.get_file_to(&full_path, &hash)
                            .await
                            .context(format!("could not get file at path: {full_path:?}"))?;
                    }
                    Change::AddSymLink(_) | Change::EditSymLink(_) => {
                        // TODO somehow use the hashes to check if the data arrived correctly
                        com.send_struct(Query::SymLinkAt(path.clone())).await?;
                        let endpoint: PathBuf = com.get_struct().await?;
                        fs::create_symlink(full_path, endpoint)?;
                    }
                    _ => {}
                }
            }

            com.send_struct(Query::Stop).await?;
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
    match (&state.update, &mut state.old_tree) {
        (Some(Commit { commit_id, delta }), Some(old_tree)) => {
            let updated_old_tree = old_tree.try_apply_delta(delta)?;

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
            *old_tree = updated_old_tree;
            let new_tree = fstree::generate_fstree(&config.link_root, &config.exclude_list)?;
            let local_delta = fstree::get_delta(&old_tree, &new_tree);

            state.new_tree = Some(new_tree);
            state.local_delta = Some(local_delta);

            fs::save(
                &config.link_root.join(".bbup").join("old-hash-tree.json"),
                &old_tree,
            )?;
            fs::save(
                &config
                    .link_root
                    .join(".bbup")
                    .join("last-known-commit.json"),
                &commit_id,
            )?;

            Ok(())
        }
        _ => {
            anyhow::bail!(
				"Some part of the state was required for apply-update but is missing\nstate.update: {}\nstate.old_tree: {}",
				state.update.is_some(),
				state.old_tree.is_some(),
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

            loop {
                let query: Query = com.get_struct().await.context("could not get query")?;
                match query {
                    Query::FileAt(path) => {
                        let full_path = config.link_root.join(&path);

                        com.send_file_from(&full_path)
                            .await
                            .context(format!("could not send file at path: {full_path:?}"))?;
                    }
                    Query::SymLinkAt(path) => {
                        let full_path = config.link_root.join(&path);

                        let symlink_endpoint = fs::read_link(&full_path)?;
                        com.send_struct(symlink_endpoint).await.context(format!(
                            "could not send symlink endpoint at path: {full_path:?}"
                        ))?;
                    }
                    Query::Stop => break,
                }
            }

            let new_commit_id: String = com.get_struct().await?;

            fs::save(
                &config.link_root.join(".bbup").join("old-hash-tree.json"),
                &new_tree,
            )?;
            fs::save(
                &config
                    .link_root
                    .join(".bbup")
                    .join("last-known-commit.json"),
                &new_commit_id,
            )?;

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
