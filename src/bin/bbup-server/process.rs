use crate::{CommmitListExt, ServerState};

use bbup_rust::{
    com::BbupCom,
    fs,
    fstree::{Change, DeltaFSNode, DeltaFSTree},
    model::{Commit, JobType, Query},
    random,
};

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::{net::TcpStream, sync::Mutex};

async fn pull<T, R>(
    com: &mut BbupCom<T, R>,
    state: &ServerState,
    endpoint: &Vec<String>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    let last_known_commit: String = com.get_struct().await.context("could not get lkc")?;

    // calculate update for client
    // TODO maybe this should panic because it means a broken server state
    let delta = state
        .commit_list
        .get_update_delta(&endpoint, last_known_commit)
        .context("could not get update delta")?;

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
        let query: Query = com.get_struct().await.context("could not get query")?;
        // let path: Option<AbstractPath> = com
        //     .get_struct()
        //     .await
        //     .context("could not get path for file to send")?;
        match query {
            Query::FileAt(path) => {
                let mut full_path = state.archive_root.clone();
                endpoint.into_iter().for_each(|comp| full_path.push(comp));
                full_path.push(&path);
                // let full_path = state.archive_root.join(&endpoint.to_path_buf()).join(&path);

                com.send_file_from(&full_path)
                    .await
                    .context(format!("could not send file at path: {full_path:?}"))?;
            }
            Query::SymLinkAt(path) => {
                let mut full_path = state.archive_root.clone();
                endpoint.into_iter().for_each(|comp| full_path.push(comp));
                full_path.push(&path);

                let symlink_endpoint = fs::read_link(&full_path)?;
                com.send_struct(symlink_endpoint).await.context(format!(
                    "could not send symlink endpoint at path: {full_path:?}"
                ))?;
            }
            Query::Stop => break,
        }
        // let path = match path {
        //     Some(val) => val,
        //     None => break,
        // };

        // let full_path = state
        //     .archive_root
        //     .join(&endpoint.to_path_buf())
        //     .join(&path.to_path_buf());

        // com.send_file_from(&full_path)
        //     .await
        //     .context(format!("could not send file at path: {full_path:?}"))?;
    }

    Ok(())
}

async fn push<T, R>(
    com: &mut BbupCom<T, R>,
    state: &mut ServerState,
    endpoint: &Vec<String>,
) -> Result<()>
where
    T: tokio::io::AsyncWrite + Unpin + Sync + Send,
    R: tokio::io::AsyncRead + Unpin + Sync + Send,
{
    fs::make_clean_dir(state.archive_root.join(".bbup").join("temp"))?;

    // Reply with green light for push
    com.send_ok()
        .await
        .context("could not send greenlight for push")?;

    // Get list of changes from client
    let mut local_delta: DeltaFSTree = com
        .get_struct()
        .await
        .context("could not get delta from client")?;

    // Get all files that need to be uploaded from client
    for (path, change) in local_delta.flatten() {
        let full_path = state.archive_root.join(".bbup").join("temp").join(&path);
        match change {
            Change::AddFile(_, _) | Change::EditFile(_, Some(_)) => {
                // com.send_struct(Some(path.clone())).await?;

                // TODO somehow use the hashes to check if the data arrived correctly
                com.send_struct(Query::FileAt(path.clone())).await?;
                com.get_file_to(&full_path)
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
    // send stop
    com.send_struct(Query::Stop)
        .await
        .context("could not send query stop to signal end of file transfer")?;

    // TODO if fail, send error message to the server
    let updated_archive_tree = state.archive_tree.try_apply_delta(&local_delta)?;

    for (path, change) in local_delta.flatten() {
        let mut to_path = state.archive_root.clone();
        endpoint.into_iter().for_each(|comp| to_path.push(comp));
        to_path.push(&path);
        let from_temp_path = state.archive_root.join(".bbup").join("temp").join(&path);

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
                fs::rename(from_temp_path, &to_path)
                    .context(errmsg("move added file from temp"))?;
                fs::set_metadata(&to_path, &metadata)
                    .context(errmsg("set metadata of added file"))?;
            }
            Change::AddSymLink(_) => {
                fs::rename(from_temp_path, &to_path)
                    .context(errmsg("move added symlink from temp"))?;
            }
            Change::EditDir(metadata) => {
                fs::set_metadata(&to_path, &metadata)
                    .context(errmsg("set metadata of edited directory"))?;
            }
            Change::EditFile(optm, opth) => {
                if let Some(_) = opth {
                    fs::rename(from_temp_path, &to_path)
                        .context(errmsg("move edited file from temp"))?;
                }
                if let Some(metadata) = optm {
                    fs::set_metadata(&to_path, &metadata)
                        .context(errmsg("set metadata of edited file"))?;
                }
            }
            Change::EditSymLink(_) => {
                fs::rename(from_temp_path, &to_path)
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
        let node = DeltaFSNode::Branch(None, local_delta.clone());
        let tree = HashMap::from([(comp.clone(), node)]);
        local_delta = DeltaFSTree(tree)
    });
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
