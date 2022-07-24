use crate::fs::{self, Metadata, ObjectType, OsStrExt, PathExt};
use crate::hash::{self, Hash};
use crate::model::ExcludeList;

use thiserror::Error;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Error, Debug)]
pub enum FSTreeError {
    #[error(
        "File System Tree Error: could not generate fs-tree from specified path as it is not a directory\npath: {path:?}"
    )]
    NonDirEntryPoint { path: PathBuf },

    #[error("File System Tree Error: inner error occurred\nSource: {src}\nError: {err}")]
    InnerError { src: String, err: String },

    #[error("File System Tree Error: some error occurred.\nSource: {src}\nError: {err}")]
    GenericError { src: String, err: String },
}

fn inerr<S: std::string::ToString, E: std::error::Error>(src: S) -> impl Fn(E) -> FSTreeError {
    move |err: E| -> FSTreeError {
        FSTreeError::InnerError {
            src: (src).to_string().clone(),
            err: err.to_string(),
        }
    }
}
fn generr<S: std::string::ToString, T: std::string::ToString>(src: S, err: T) -> FSTreeError {
    FSTreeError::GenericError {
        src: (src).to_string().clone(),
        err: err.to_string(),
    }
}
fn error_context<S: std::string::ToString>(context: S) -> impl Fn(&str) -> String {
    move |failure: &str| -> String { format!("{}\nFailed to {}", context.to_string(), failure) }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FSNode {
    File(Metadata, Hash),
    SymLink(Hash),
    Dir(Metadata, Hash, FSTree),
}
impl PartialEq for FSNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(metadata_l, hash_l), Self::File(metadata_r, hash_r)) => {
                metadata_l == metadata_r && hash_l == hash_r
            }
            (Self::SymLink(hash_l), Self::SymLink(hash_r)) => hash_l == hash_r,

            // Do not check for subtree structure: the idea is that the hash represents
            //	itself the tree structure, so the trees are equal iff the hashes are
            //	equal, hence check the hash and not the subtree
            (Self::Dir(metadata_l, hash_l, _), Self::Dir(metadata_r, hash_r, _)) => {
                metadata_l == metadata_r && hash_l == hash_r
            }

            _ => false,
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct FSTree(pub HashMap<String, FSNode>);
impl FSTree {
    pub fn empty() -> FSTree {
        FSTree(HashMap::new())
    }
}

/// Hash the endpoint of a symlink
fn hash_symlink<P: AsRef<Path>>(path: P) -> Result<Hash, FSTreeError> {
    let errctx = error_context(format!(
        "could not hash content of file at path {:?}",
        path.as_ref()
    ));
    let endpoint = fs::read_link(path).map_err(inerr(errctx("read symlink's endpoint")))?;
    Ok(hash::hash_bytes(endpoint.force_to_string().as_bytes()))
}
/// Hash the content of a file
fn hash_file<P: AsRef<Path>>(path: P) -> Result<Hash, FSTreeError> {
    let errctx = error_context(format!(
        "could not hash content of file at path {:?}",
        path.as_ref()
    ));
    let content = fs::read_file(path).map_err(inerr(errctx("read file content")))?;
    hash::hash_stream(content).map_err(inerr(errctx("hash file content")))
}
/// Hash children of a node by concatenating their names and their relative hashes
pub fn hash_tree(FSTree(tree): &FSTree) -> Hash {
    let mut sorted_children = tree.into_iter().collect::<Vec<(&String, &FSNode)>>();
    sorted_children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));

    let mut s: Vec<u8> = Vec::new();
    for (name, node) in sorted_children {
        s.append(&mut name.as_bytes().to_vec());
        match node {
            FSNode::File(metadata, hash) => {
                s.append(&mut metadata.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            FSNode::SymLink(hash) => {
                s.append(&mut hash.to_bytes());
            }
            FSNode::Dir(metadata, hash, _) => {
                s.append(&mut metadata.to_bytes());
                s.append(&mut hash.to_bytes());
            }
        }
    }
    hash::hash_bytes(s)
}

/// Generate a tree representation of the content of a path specified, saving the hashes
/// at every node and leaf to be able to detect changes
pub fn generate_fstree<P: AsRef<Path>>(
    root: P,
    exclude_list: &ExcludeList,
) -> Result<FSTree, FSTreeError> {
    let root = root.as_ref().to_path_buf();
    if root.get_type() != ObjectType::Dir {
        return Err(FSTreeError::NonDirEntryPoint { path: root.clone() });
    }
    generate_fstree_rec(&root, &PathBuf::from("."), exclude_list)
}

fn generate_fstree_rec(
    path: &PathBuf,
    rel_path: &PathBuf,
    exclude_list: &ExcludeList,
) -> Result<FSTree, FSTreeError> {
    let errctx = error_context(format!(
        "could not generate fstree from subtree at path {:?}",
        path
    ));
    let mut tree: HashMap<String, FSNode> = HashMap::new();

    let read_dir_instance =
        fs::list_dir_content(&path).map_err(inerr(errctx("list content of dir")))?;
    for entry in read_dir_instance {
        let file_name = entry
            .file_name()
            .ok_or(generr(
                errctx(format!("get filename of child at path {:?}", entry).as_str()),
                "child path might be ending in `..`",
            ))?
            .force_to_string();
        let rel_subpath = rel_path.join(&file_name);
        // TODO should exclude 2?!?!?!
        if exclude_list.should_exclude(&rel_subpath, entry.is_dir()) {
            continue;
        }

        // let metadata = get_metadata(&entry)?;
        let node = match entry.get_type() {
            ObjectType::Dir => {
                let metadata = fs::get_metadata(&entry).map_err(inerr(errctx(
                    format!("get metadata of dir at path {:?}", entry).as_str(),
                )))?;
                let subtree = generate_fstree_rec(&entry, &rel_subpath, exclude_list)?;
                let hash = hash_tree(&subtree);
                FSNode::Dir(metadata, hash, subtree)
            }
            ObjectType::File => {
                let metadata = fs::get_metadata(&entry).map_err(inerr(errctx(
                    format!("get metadata of file at path {:?}", entry).as_str(),
                )))?;
                let hash = hash_file(&entry).map_err(inerr(errctx(
                    format!("hash file at path {:?}", entry).as_str(),
                )))?;
                FSNode::File(metadata, hash)
            }
            ObjectType::SymLink => {
                let hash = hash_symlink(&entry).map_err(inerr(errctx(
                    format!("hash symlink at path {:?}", entry).as_str(),
                )))?;
                FSNode::SymLink(hash)
            }
        };

        tree.insert(file_name, node);
    }

    Ok(FSTree(tree))
}
