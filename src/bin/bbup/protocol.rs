use bbup_rust::path::AbstractPath;
use std::path::PathBuf;

use crate::{ProcessConfig, ProcessState};

use bbup_rust::com::BbupCom;
use bbup_rust::model::{Adding, ChangeType, Commit, Editing, PrettyPrint, Removing};
use bbup_rust::{fs, hashtree, utils};

use anyhow::{Context, Result};

pub fn get_local_delta(config: &ProcessConfig, state: &mut ProcessState) -> Result<()> {
    if config.flags.verbose {
        println!("calculating local delta...")
    }

    state.last_known_commit = Some(fs::load(&config.lkc_path())?);

    let old_tree = fs::load(&config.old_tree_path())?;
    let new_tree = hashtree::generate_hash_tree(&config.link_root, &config.exclude_list)?;
    let local_delta = hashtree::delta(&old_tree, &new_tree);

    if config.flags.verbose {
        if local_delta.len() == 0 {
            println!("local delta: no local changes to push")
        } else {
            println!("local delta:\n{}", local_delta.pretty_print(1))
        }
    }

    state.old_tree = Some(old_tree);
    state.new_tree = Some(new_tree);
    state.local_delta = Some(local_delta);
    Ok(())
}

pub async fn pull_update_delta<T, R>(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom<T, R>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
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
            update.delta.retain(|change| !match change.change_type {
                ChangeType::Added(Adding::Dir) | ChangeType::Removed(Removing::Dir) => {
                    config.exclude_list.should_exclude(&change.path, true)
                }
                _ => config.exclude_list.should_exclude(&change.path, false),
            });

            if config.flags.verbose {
                if update.delta.len() == 0 {
                    println!("pull delta: no missed change to pull")
                } else {
                    println!("pull delta:\n{}", update.delta.pretty_print(1))
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
            let mut conflicts: Vec<(String, String)> = Vec::new();

            local_delta.into_iter().for_each(|local_change| {
                update_delta.into_iter().for_each(|update_change| {
                    let is_conflict = {
                        if local_change.path.eq(&update_change.path) {
                            match (&local_change.change_type, &update_change.change_type) {
                                (
                                    ChangeType::Added(Adding::Dir),
                                    ChangeType::Added(Adding::Dir),
                                )
                                | (
                                    ChangeType::Removed(Removing::Dir),
                                    ChangeType::Removed(Removing::Dir),
                                ) => false,

                                (
                                    ChangeType::Added(Adding::FileType(type0, hash0)),
                                    ChangeType::Added(Adding::FileType(type1, hash1)),
                                )
                                | (
                                    ChangeType::Edited(Editing::FileType(type0, hash0)),
                                    ChangeType::Edited(Editing::FileType(type1, hash1)),
                                ) if type0 == type1 && hash0 == hash1 => false,

                                (
                                    ChangeType::Removed(Removing::FileType(type0)),
                                    ChangeType::Removed(Removing::FileType(type1)),
                                ) if type0 == type1 => false,

                                _ => true,
                            }
                        } else {
                            local_change.path.starts_with(&update_change.path)
                                || update_change.path.starts_with(&local_change.path)
                        }
                    };

                    if is_conflict {
                        // TODO: make the conflic explanation a little bit better
                        conflicts.push((
                            format!("local_change:  {:?}", local_change.path),
                            format!("update change: {:?}", update_change.path),
                        ));
                    }
                })
            });
            if conflicts.len() > 0 {
                println!("conflicts:");
                conflicts
                    .into_iter()
                    .for_each(|s| println!("\t!!! {}\n\t    {}", s.0, s.1));
                return Err(anyhow::Error::new(utils::std_err(
                    "found conflicts between pulled update and local changes. Resolve manually",
                )));
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

pub async fn download_update<T, R>(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom<T, R>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    match &state.update {
        Some(Commit {
            commit_id: _,
            delta: update_delta,
        }) => {
            // Get all files that need to be downloaded from server
            for change in update_delta {
                match change.change_type {
                    ChangeType::Added(Adding::FileType(_, _)) | ChangeType::Edited(_) => {
                        com.send_struct(Some(change.path.clone())).await?;

                        let full_path = &config.local_temp_path().join(change.path.to_path_buf());
                        com.get_file_to(&full_path)
                            .await
                            .context(format!("could not get file at path: {full_path:?}"))?;
                    }
                    _ => {}
                };
            }

            com.send_struct(None::<PathBuf>).await?;
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

            for change in delta {
                let path = config.link_root.join(&change.path.to_path_buf());
                let from_temp_path = config.local_temp_path().join(&change.path.to_path_buf());
                match change.change_type {
                    ChangeType::Added(Adding::Dir) => {
                        std::fs::create_dir(&path).context(format!(
                            "could not create directory to apply update\npath: {:?}",
                            path
                        ))?
                    }
                    ChangeType::Added(Adding::FileType(_, _)) | ChangeType::Edited(_) => {
                        std::fs::rename(&from_temp_path, &path).context(format!(
                            "could not copy file from temp to apply update\npath: {:?}",
                            path
                        ))?;
                    }
                    ChangeType::Removed(Removing::Dir) => {
                        std::fs::remove_dir(&path).context(format!(
                            "could not remove directory to apply update\npath: {:?}",
                            path
                        ))?
                    }
                    ChangeType::Removed(Removing::FileType(_)) => std::fs::remove_file(&path)
                        .context(format!(
                            "could not remove file to apply update\npath: {:?}",
                            path
                        ))?,
                }
            }
            *old_tree = updated_old_tree;
            let new_tree = hashtree::generate_hash_tree(&config.link_root, &config.exclude_list)?;
            let local_delta = hashtree::delta(&old_tree, &new_tree);

            state.new_tree = Some(new_tree);
            state.local_delta = Some(local_delta);

            fs::save(&config.old_tree_path(), &old_tree)?;
            fs::save(&config.lkc_path(), &commit_id)?;

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

pub async fn upload_changes<T, R>(
    config: &ProcessConfig,
    state: &mut ProcessState,
    com: &mut BbupCom<T, R>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    match (&state.local_delta, &state.new_tree) {
        (Some(local_delta), Some(new_tree)) => {
            // Await green light to procede
            com.check_ok().await?;

            com.send_struct(local_delta).await?;

            loop {
                let path: Option<AbstractPath> = com.get_struct().await?;
                let path = match path {
                    Some(val) => val,
                    None => break,
                };
                com.send_file_from(&config.link_root.join(path.to_path_buf()))
                    .await?;
            }

            let new_commit_id: String = com.get_struct().await?;

            fs::save(&config.old_tree_path(), &new_tree)?;
            fs::save(&config.lkc_path(), &new_commit_id)?;

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
