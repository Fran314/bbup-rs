use crate::fs::{self, AbstPath, Mtime, ObjectType};
use crate::hash::{self, Hash};
use crate::model::ExcludeList;

use thiserror::Error;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

#[derive(Error, Debug)]
pub enum FSTreeError {
    #[error(
        "File System Tree Error: could not generate fs-tree from specified path as it is not a directory\npath: {path}"
    )]
    NonDirEntryPoint { path: AbstPath },

    #[error("File System Tree Error: inner error occurred\nSource: {src}\nError: {err}")]
    Inner { src: String, err: String },

    #[error("File System Tree Error: some error occurred.\nSource: {src}\nError: {err}")]
    Generic { src: String, err: String },
}

fn inerr<S: std::string::ToString, E: std::error::Error>(src: S) -> impl Fn(E) -> FSTreeError {
    move |err: E| -> FSTreeError {
        FSTreeError::Inner {
            src: src.to_string(),
            err: err.to_string(),
        }
    }
}
fn generr<S: std::string::ToString, T: std::string::ToString>(src: S, err: T) -> FSTreeError {
    FSTreeError::Generic {
        src: src.to_string(),
        err: err.to_string(),
    }
}
fn error_context<S: std::string::ToString>(context: S) -> impl Fn(&str) -> String {
    move |failure: &str| -> String { format!("{}\nFailed to {}", context.to_string(), failure) }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FSNode {
    File(Mtime, Hash),
    SymLink(Mtime, Hash),
    Dir(Mtime, Hash, FSTree),
}
impl PartialEq for FSNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(mtime_l, hash_l), Self::File(mtime_r, hash_r))
            | (Self::SymLink(mtime_l, hash_l), Self::SymLink(mtime_r, hash_r))

            // Do not check for subtree structure: the idea is that the hash represents
            //	itself the tree structure, so the trees are equal iff the hashes are
            //	equal, hence check the hash and not the subtree
            | (Self::Dir(mtime_l, hash_l, _), Self::Dir(mtime_r, hash_r, _)) => {
                mtime_l == mtime_r && hash_l == hash_r
            }

            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FSTree(pub HashMap<String, FSNode>);
impl FSTree {
    pub fn empty() -> FSTree {
        FSTree(HashMap::new())
    }
}

/// Hash the endpoint of a symlink
fn hash_symlink(path: &AbstPath) -> Result<Hash, FSTreeError> {
    let errctx = error_context(format!("could not hash content of file at path {path}"));
    let endpoint = fs::read_link(path).map_err(inerr(errctx("read symlink's endpoint")))?;
    Ok(hash::hash_bytes(endpoint.as_bytes()))
}
/// Hash the content of a file
fn hash_file(path: &AbstPath) -> Result<Hash, FSTreeError> {
    let errctx = error_context(format!("could not hash content of file at path {path}"));
    let content = fs::read_file(path).map_err(inerr(errctx("read file content")))?;
    hash::hash_stream(content).map_err(inerr(errctx("hash file content")))
}
/// Hash children of a node by concatenating their names and their relative hashes
pub fn hash_tree(FSTree(tree): &FSTree) -> Hash {
    let mut sorted_children = tree.iter().collect::<Vec<(&String, &FSNode)>>();
    sorted_children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));

    let mut s: Vec<u8> = Vec::new();
    for (name, node) in sorted_children {
        s.append(&mut name.as_bytes().to_vec());
        match node {
            FSNode::File(mtime, hash) => {
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            FSNode::SymLink(mtime, hash) => {
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            FSNode::Dir(mtime, hash, _) => {
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
        }
    }
    hash::hash_bytes(s)
}

/// Generate a tree representation of the content of a path specified, saving the hashes
/// at every node and leaf to be able to detect changes
pub fn generate_fstree(root: &AbstPath, exclude_list: &ExcludeList) -> Result<FSTree, FSTreeError> {
    if root.object_type() != Some(ObjectType::Dir) {
        return Err(FSTreeError::NonDirEntryPoint { path: root.clone() });
    }
    generate_fstree_rec(root, &AbstPath::single("."), exclude_list)
}

fn generate_fstree_rec(
    path: &AbstPath,
    rel_path: &AbstPath,
    exclude_list: &ExcludeList,
) -> Result<FSTree, FSTreeError> {
    let errctx = error_context(format!(
        "could not generate fstree from subtree at path {path}"
    ));
    let mut tree: HashMap<String, FSNode> = HashMap::new();

    let read_dir_instance =
        fs::list_dir_content(path).map_err(inerr(errctx("list content of dir")))?;
    for entry in read_dir_instance {
        let objec_type = entry.object_type().ok_or_else(|| {
            generr(
                errctx(format!("get type of child at path {entry}").as_str()),
                "child should exist but doesn't have a type (as if it doesn't exist)",
            )
        })?;
        let file_name = entry.file_name().ok_or_else(|| {
            generr(
                errctx(format!("get filename of child at path {entry}").as_str()),
                "child path might be ending in `..`",
            )
        })?;
        let rel_subpath = rel_path.add_last(&file_name);
        if exclude_list.should_exclude(&rel_subpath, objec_type == ObjectType::Dir) {
            continue;
        }

        let node = match objec_type {
            ObjectType::Dir => {
                let mtime = fs::get_mtime(&entry).map_err(inerr(errctx(
                    format!("get mtime of dir at path {entry}").as_str(),
                )))?;
                let subtree = generate_fstree_rec(&entry, &rel_subpath, exclude_list)?;
                let hash = hash_tree(&subtree);
                FSNode::Dir(mtime, hash, subtree)
            }
            ObjectType::File => {
                let mtime = fs::get_mtime(&entry).map_err(inerr(errctx(
                    format!("get mtime of file at path {entry}").as_str(),
                )))?;
                let hash = hash_file(&entry)
                    .map_err(inerr(errctx(format!("hash file at path {entry}").as_str())))?;
                FSNode::File(mtime, hash)
            }
            ObjectType::SymLink => {
                let mtime = fs::get_mtime(&entry).map_err(inerr(errctx(
                    format!("get mtime of symlink at path {entry}").as_str(),
                )))?;
                let hash = hash_symlink(&entry).map_err(inerr(errctx(
                    format!("hash symlink at path {entry}").as_str(),
                )))?;
                FSNode::SymLink(mtime, hash)
            }
        };

        tree.insert(file_name, node);
    }

    Ok(FSTree(tree))
}
