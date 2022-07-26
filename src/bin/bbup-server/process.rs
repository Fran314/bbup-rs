use crate::{ArchiveConfig, ArchiveState, CommmitListExt};

use bbup_rust::{
    com::{BbupCom, JobType, Querable},
    fs,
    fstree::{Change, Delta, DeltaNode},
    hash::Hash,
    model::Commit,
    random,
};

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::{net::TcpStream, sync::Mutex};

async fn pull(
    config: &ArchiveConfig,
    state: &ArchiveState,
    com: &mut BbupCom,
    endpoint: &Vec<String>,
) -> Result<()> {
    let last_known_commit: String = com.get_struct().await.context("could not get lkc")?;

    // calculate update for client
    // TODO maybe this should panic because it means a broken server state
    let delta = state
        .commit_list
        .get_update_delta(&endpoint, last_known_commit)
        .context("could not get update delta")?;

    let querable: Vec<PathBuf> = delta
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
    let source = {
        let mut path = config.archive_root.clone();
        endpoint.into_iter().for_each(|comp| path.push(comp));
        path
    };
    com.supply_files(&querable, &source)
        .await
        .context("could not supply files to download update")?;

    Ok(())
}

async fn push(
    config: &ArchiveConfig,
    state: &mut ArchiveState,
    com: &mut BbupCom,
    endpoint: &Vec<String>,
) -> Result<()> {
    fs::make_clean_dir(config.archive_root.join(".bbup").join("temp"))?;

    // Reply with green light for push
    com.send_ok()
        .await
        .context("could not send greenlight for push")?;

    // Get list of changes from client
    let mut local_delta: Delta = com
        .get_struct()
        .await
        .context("could not get delta from client")?;

    // Get all files that need to be uploaded from client
    let queries: Vec<(Querable, PathBuf, Hash)> = local_delta
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
    com.query_files(queries, &config.archive_root.join(".bbup").join("temp"))
        .await
        .context("could not query files to apply push")?;

    // TODO if fail, send error message to the server
    let updated_archive_tree = state.archive_tree.try_apply_delta(&local_delta)?;

    for (path, change) in local_delta.flatten() {
        let mut to_path = config.archive_root.clone();
        endpoint.into_iter().for_each(|comp| to_path.push(comp));
        to_path.push(&path);
        let from_temp_path = config.archive_root.join(".bbup").join("temp").join(&path);

        let errmsg = |msg: &str| -> String {
            format!(
                "could not {} to apply new commit\npath: {:?}",
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

    endpoint.into_iter().rev().for_each(|comp| {
        let node = DeltaNode::Branch(None, local_delta.clone());
        let tree = HashMap::from([(comp.clone(), node)]);
        local_delta = Delta(tree)
    });
    let commit_id = random::random_hex(64);
    state.commit_list.push(Commit {
        commit_id: commit_id.clone(),
        delta: local_delta.clone(),
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

        let endpoint: Vec<String> = com
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
