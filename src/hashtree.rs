use crate::path::*;
use crate::structs::{Adding, Change, ChangeType, Delta, Editing, Removing};

use thiserror::Error;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::path::PathBuf;

use base64ct::{Base64, Encoding};
use regex::Regex;
use sha2::{Digest, Sha256};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Could not open path\npath: {path:?}\ninfo: {info}")]
    FsError { path: PathBuf, info: String },

    #[error("Could not copy writer into reader\ninfo: {info}")]
    CopyError { info: String },

    #[error("Trying to generate hash tree from a path corresponding to something that is not a directory. While technically possible, this is undesireable behaviour\npath: {path:?}")]
    LeafOnlyTree { path: PathBuf },

    #[error("Could not apply change to tree\ninfo: {info}")]
    ApplyChangeError { info: String },
}

pub struct ExcludeList {
    list: Vec<Regex>,
}
impl ExcludeList {
    pub fn from(list: &Vec<Regex>) -> ExcludeList {
        ExcludeList { list: list.clone() }
    }
    pub fn should_exclude(&self, path: &AbstractPath, is_dir: bool) -> bool {
        let path_as_string = {
            let mut tmp = path.to_string();
            if is_dir {
                tmp.push(std::path::MAIN_SEPARATOR);
            }
            tmp
        };

        for rule in &self.list {
            if rule.is_match(path_as_string.as_str()) {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Tree {
    Node {
        hash: String,
        children: HashMap<String, Tree>,
    },
    Leaf {
        hash: String,
        leaf_type: FileType,
    },
}

#[derive(PartialEq)]
enum Traverse {
    Pre,
    Post,
}

/// Hash anything that can be converted to u8 array (usually
/// Strings or &str) and convert to base64
fn hash_string<T: std::convert::AsRef<[u8]>>(s: T) -> String {
    let hash = Sha256::digest(s);
    Base64::encode_string(&hash)
}

/// Hash anything that can be streamed (usually files)
/// and convert to base64
fn hash_stream<T: std::io::Read>(mut stream: T) -> Result<String, Error> {
    let mut hasher = Sha256::new();
    match std::io::copy(&mut stream, &mut hasher) {
        Ok(_) => {}
        Err(error) => {
            return Err(Error::CopyError {
                info: error.to_string(),
            });
        }
    };
    let hash = hasher.finalize();

    Ok(Base64::encode_string(&hash))
}

/// Hash children of a node by concatenating their names
/// and their relative hashes, and convert to base64
fn hash_children(children: &HashMap<String, Tree>) -> String {
    let mut s = String::new();
    for (child_name, child_node) in children {
        s += child_name.as_str();
        s += match child_node {
            Tree::Node { hash, children: _ } | Tree::Leaf { hash, leaf_type: _ } => hash.as_str(),
        };
    }
    hash_string(s)
}

/// Hash the content of a file and convert to base64
fn hash_file(path: &PathBuf) -> Result<String, Error> {
    match std::fs::File::open(path) {
        Ok(file) => hash_stream(file),
        Err(error) => Err(Error::FsError {
            path: path.clone(),
            info: error.to_string(),
        }),
    }
}

/// Hash the link of a symlink and convert to base64
fn hash_symlink(path: &PathBuf) -> Result<String, Error> {
    match std::fs::read_link(path) {
        Ok(link) => Ok(hash_string(link.as_os_str().force_to_string())),
        Err(error) => Err(Error::FsError {
            path: path.clone(),
            info: error.to_string(),
        }),
    }
}

/// Generate a tree representation of the content of a path
/// specified, saving the hashes at every node and leaf to
/// be able to detect changes
pub fn generate_hash_tree(root: &PathBuf, exclude_list: &ExcludeList) -> Result<Tree, Error> {
    if root.get_type() != EntryType::Dir {
        return Err(Error::LeafOnlyTree { path: root.clone() });
    }
    generate_hash_tree_rec(root, &AbstractPath::empty(), exclude_list)
}

fn generate_hash_tree_rec(
    path: &PathBuf,
    rel_path: &AbstractPath,
    exclude_list: &ExcludeList,
) -> Result<Tree, Error> {
    let output: Tree;

    match path.get_type() {
        EntryType::Dir => {
            let mut children: HashMap<String, Tree> = HashMap::new();
            let read_dir_instance = match std::fs::read_dir(&path) {
                Ok(val) => val,
                Err(error) => {
                    return Err(Error::FsError {
                        path: path.clone(),
                        info: error.to_string(),
                    });
                }
            };
            for entry in read_dir_instance {
                let entry = match entry {
                    Ok(val) => val,
                    Err(error) => {
                        return Err(Error::FsError {
                            path: path.clone(),
                            info: error.to_string(),
                        });
                    }
                };
                let entry_path = entry.path().to_path_buf();
                let is_dir = entry_path.is_dir();
                let file_name = entry_path.force_file_name();
                let rel_subpath = {
                    let mut temp = rel_path.clone();
                    temp.push_back(file_name.clone());
                    temp
                };

                if !exclude_list.should_exclude(&rel_subpath, is_dir) {
                    children.insert(
                        file_name,
                        generate_hash_tree_rec(&entry_path, &rel_subpath, exclude_list)?,
                    );
                }
            }
            let hash = hash_children(&children);
            output = Tree::Node { hash, children };
        }
        EntryType::FileType(FileType::File) => {
            let hash = hash_file(path)?;
            output = Tree::Leaf {
                hash,
                leaf_type: FileType::File,
            };
        }
        EntryType::FileType(FileType::SymLink) => {
            let hash = hash_symlink(path)?;
            output = Tree::Leaf {
                hash,
                leaf_type: FileType::SymLink,
            };
        }
    }

    Ok(output)
}

fn get_both_keys<T: Clone + Eq + std::hash::Hash, S>(
    arg0: &HashMap<T, S>,
    arg1: &HashMap<T, S>,
) -> Vec<T> {
    let mut output_hm: HashMap<T, bool> = HashMap::new();
    arg0.keys().for_each(|el| {
        output_hm.insert(el.clone(), false);
    });
    arg1.keys().for_each(|el| {
        output_hm.insert(el.clone(), false);
    });
    Vec::from_iter(output_hm.keys().into_iter().map(|el| el.clone()))
}

fn traverse(tree: &Tree, trav: &Traverse) -> Vec<(AbstractPath, EntryType, String)> {
    match tree {
        Tree::Node { hash, children } => {
            let mut output: Vec<(AbstractPath, EntryType, String)> = Vec::new();

            if trav.eq(&Traverse::Pre) {
                // Add node pre-visit
                output.push((AbstractPath::empty(), EntryType::Dir, hash.clone()));
            }

            // Visit children
            for (child_name, child) in children {
                for mut node in traverse(child, trav) {
                    node.0.push_front(child_name.clone());
                    output.push(node);
                }
            }

            if trav.eq(&Traverse::Post) {
                // Add node post-visit
                output.push((AbstractPath::empty(), EntryType::Dir, hash.clone()));
            }

            return output;
        }
        Tree::Leaf { hash, leaf_type } => {
            return Vec::from([(
                AbstractPath::empty(),
                EntryType::FileType(leaf_type.clone()),
                hash.clone(),
            )]);
        }
    }
}
fn add_whole_tree(tree: &Tree) -> Delta {
    let node_list = traverse(&tree, &Traverse::Pre);
    node_list
        .into_iter()
        .map(|(path, entry_type, hash)| {
            let add = match entry_type {
                EntryType::Dir => Adding::Dir,
                EntryType::FileType(t) => Adding::FileType(t, hash),
            };
            Change {
                path,
                change_type: ChangeType::Added(add),
            }
        })
        .collect()
}
fn remove_whole_tree(tree: &Tree) -> Delta {
    let node_list = traverse(&tree, &Traverse::Post);
    node_list
        .into_iter()
        .map(|(path, entry_type, _)| {
            let remove = match entry_type {
                EntryType::Dir => Removing::Dir,
                EntryType::FileType(t) => Removing::FileType(t),
            };
            Change {
                path,
                change_type: ChangeType::Removed(remove),
            }
        })
        .collect()
}
fn with_prefix(prefix: &String, delta: Delta) -> Delta {
    delta
        .into_iter()
        .map(|mut change| {
            change.path.push_front(prefix.clone());
            change
        })
        .collect()
}

pub fn delta(old_tree: &Tree, new_tree: &Tree) -> Delta {
    let mut output: Delta = Vec::new();

    let old_hash = match old_tree {
        Tree::Node { hash, children: _ } | Tree::Leaf { hash, leaf_type: _ } => hash,
    };
    let new_hash = match new_tree {
        Tree::Node { hash, children: _ } | Tree::Leaf { hash, leaf_type: _ } => hash,
    };

    if old_hash.eq(new_hash) {
        return output;
    }

    match (old_tree, new_tree) {
        // Edited some content of a folder
        (
            Tree::Node {
                hash: _,
                children: old_children,
            },
            Tree::Node {
                hash: _,
                children: new_children,
            },
        ) => {
            for key in get_both_keys(&old_children, &new_children) {
                match (old_children.get(&key), new_children.get(&key)) {
					(Some(child0), None) => {
						output.append(&mut with_prefix(&key, remove_whole_tree(child0)));
					},
					(None, Some(child1)) => {
						output.append(&mut with_prefix(&key, add_whole_tree(child1)));
					},
					(Some(child0), Some(child1)) => {
						output.append(&mut with_prefix(&key, delta(child0, child1)));
					},
					(None, None) => unreachable!("Unexpected error upon set union: an element in the set union does not belong in either of the two original sets"),
				}
            }
        }

        // Edited a leaf
        (
            Tree::Leaf {
                hash: _,
                leaf_type: old_type,
            },
            Tree::Leaf {
                hash: _,
                leaf_type: new_type,
            },
        ) if old_type == new_type => {
            output.push(Change {
                path: AbstractPath::empty(),
                change_type: ChangeType::Edited(Editing::FileType(
                    old_type.clone(),
                    new_hash.clone(),
                )),
            });
        }

        // Remaining options are:
        //	- Overwrote a leaf with new one of different type
        //	- Overwrote a node with a leaf
        //	- Overwrote a leaf with a node
        // In any case, the procedure is to remove the whole old tree (vec of one element if it was a leaf, vec of all subnodes-and-leaves if node)
        //	and to add the whole new tree
        _ => {
            output.append(&mut remove_whole_tree(old_tree));
            output.append(&mut add_whole_tree(new_tree));
        }
    }

    output
}

impl Tree {
    pub fn empty_node() -> Tree {
        Tree::Node {
            hash: hash_string(""),
            children: HashMap::new(),
        }
    }
    pub fn apply_delta(&mut self, delta: &Delta) -> Result<(), Error> {
        for change in delta {
            self.apply_change(change.clone())?
        }

        Ok(())
    }

    fn apply_change(&mut self, mut change: Change) -> Result<(), Error> {
        match self {
            Tree::Leaf {
                hash: _,
                leaf_type: _,
            } => Err(Error::ApplyChangeError {
                info: "cannot apply change on a root-only tree (leaf)".to_string(),
            }),
            Tree::Node { hash: _, children } => {
                if change.path.size() == 0 {
                    return Err(Error::ApplyChangeError {
                        info: "cannot apply change intended on root".to_string(),
                    });
                } else if change.path.size() == 1 {
                    // Unwrap is ok because I know the length of path is at least 2
                    //	so pop_back will not fail
                    let curr_component = change.path.pop_back().unwrap();
                    let child = children.get_mut(&curr_component);
                    match (change.change_type, child) {
                        // Adding a directory
                        (ChangeType::Added(Adding::Dir), None) => {
                            children.insert(curr_component, Tree::empty_node());
                        }

                        // Adding a filelike
                        (ChangeType::Added(Adding::FileType(file_type, hash)), None) => {
                            children.insert(
                                curr_component,
                                Tree::Leaf {
                                    hash,
                                    leaf_type: file_type,
                                },
                            );
                        }

                        // Editing a filelike
                        (
                            ChangeType::Edited(Editing::FileType(file_type, edit_hash)),
                            Some(Tree::Leaf { hash: _, leaf_type }),
                        ) if file_type.eq(leaf_type) => {
                            children.insert(
                                curr_component,
                                Tree::Leaf {
                                    hash: edit_hash,
                                    leaf_type: file_type,
                                },
                            );
                        }

                        // Removing a directory
                        (
                            ChangeType::Removed(Removing::Dir),
                            Some(Tree::Node {
                                hash: _,
                                children: _,
                            }),
                        ) => {
                            // TODO maybe check if directory is empty (aka no children)
                            children.remove(&curr_component);
                        }

                        // Removing a filelike
                        (
                            ChangeType::Removed(Removing::FileType(file_type)),
                            Some(Tree::Leaf { hash: _, leaf_type }),
                        ) if file_type.eq(leaf_type) => {
                            children.remove(&curr_component);
                        }

                        _ => {
                            return Err(Error::ApplyChangeError {
                                info: "Cannot apply change, change inconsistent with tree"
                                    .to_string(),
                            });
                        }
                    }
                } else {
                    // Unwrap is ok because I know the length of path is at least 2
                    //	so pop_back will not fail
                    let curr_component = change.path.pop_back().unwrap();
                    let child = children.get_mut(&curr_component);
                    match child {
                        Some(child) => child.apply_change(change)?,
                        None => {
                            return Err(Error::ApplyChangeError {
                                info: "Cannot follow path to apply change: child doesn't exist"
                                    .to_string(),
                            });
                        }
                    }
                }

                Ok(())
            }
        }
    }
}
