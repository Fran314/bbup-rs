use super::{hash_tree, union, FSNode, FSTree, IOr};

use crate::{fs::Metadata, model::ExcludeList};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeltaError {
    #[error("File System Tree Delta: unable to apply delta to tree")]
    InapplicableDelta,

    #[error("File System Tree Delta: inner error occurred\nSource: {src}\nError: {err}")]
    InnerError { src: String, err: String },

    #[error("File System Tree Delta: some error occurred.\nSource: {src}\nError: {err}")]
    GenericError { src: String, err: String },
}

#[derive(Error, Debug)]
#[error(
    "File System Tree Delta: unable to apply delta to tree.\nConflict at path: {0}\nError: {1}"
)]
pub struct InapplicableDelta(String, String);
fn inapperr<S: std::string::ToString, T: std::string::ToString>(
    path: S,
    err: T,
) -> InapplicableDelta {
    InapplicableDelta(path.to_string(), err.to_string())
}
fn push_inapp<S: std::string::ToString>(
    parent: S,
) -> impl Fn(InapplicableDelta) -> InapplicableDelta {
    move |InapplicableDelta(path, err)| {
        InapplicableDelta(parent.to_string() + "/" + path.as_str(), err)
    }
}

#[derive(Error, Debug)]
#[error("File System Tree Delta: unable to merge deltas.\nConflict at path: {0}\nError: {1}")]
pub struct UnmergeableDelta(String, String);
fn unmergerr<S: std::string::ToString, T: std::string::ToString>(
    path: S,
    err: T,
) -> UnmergeableDelta {
    UnmergeableDelta(path.to_string(), err.to_string())
}
fn push_unmerg<S: std::string::ToString>(
    parent: S,
) -> impl Fn(UnmergeableDelta) -> UnmergeableDelta {
    move |UnmergeableDelta(path, err)| {
        UnmergeableDelta(parent.to_string() + "/" + path.as_str(), err)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DeltaFSNode {
    Leaf(Option<FSNode>, Option<FSNode>),
    Branch(Option<(Metadata, Metadata)>, DeltaFSTree),
}
impl DeltaFSNode {
    fn remove(pre: &FSNode) -> DeltaFSNode {
        DeltaFSNode::Leaf(Some(pre.clone()), None)
    }
    fn add(post: &FSNode) -> DeltaFSNode {
        DeltaFSNode::Leaf(None, Some(post.clone()))
    }
    fn edit(pre: &FSNode, post: &FSNode) -> Option<DeltaFSNode> {
        if pre != post {
            Some(DeltaFSNode::Leaf(Some(pre.clone()), Some(post.clone())))
        } else {
            None
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DeltaFSTree(pub HashMap<String, DeltaFSNode>);
impl DeltaFSTree {
    pub fn empty() -> DeltaFSTree {
        DeltaFSTree(HashMap::new())
    }
    pub fn is_empty(&self) -> bool {
        let DeltaFSTree(hashmap) = self;
        hashmap.len() == 0
    }
    pub fn get_subdelta_tree_copy(&self, path: &Vec<String>) -> Option<DeltaFSTree> {
        let DeltaFSTree(tree) = self;
        if path.len() == 0 {
            return Some(self.clone());
        }
        match tree.get(&path[0]) {
            None => None,
            Some(DeltaFSNode::Branch(_, subdelta)) => {
                subdelta.get_subdelta_tree_copy(&path[1..].to_vec())
            }
            Some(DeltaFSNode::Leaf(None, Some(FSNode::Dir(_, _, FSTree(subtree))))) => {
                let mut subdelta: HashMap<String, DeltaFSNode> = HashMap::new();
                for (node, child) in subtree {
                    subdelta.insert(node.clone(), DeltaFSNode::Leaf(None, Some(child.clone())));
                }
                DeltaFSTree(subdelta).get_subdelta_tree_copy(&path[1..].to_vec())
            }
            Some(DeltaFSNode::Leaf(Some(FSNode::Dir(_, _, FSTree(subtree))), None)) => {
                let mut subdelta: HashMap<String, DeltaFSNode> = HashMap::new();
                for (node, child) in subtree {
                    subdelta.insert(node.clone(), DeltaFSNode::Leaf(Some(child.clone()), None));
                }
                DeltaFSTree(subdelta).get_subdelta_tree_copy(&path[1..].to_vec())
            }
            Some(DeltaFSNode::Leaf(_, _)) => None,
        }
    }
    pub fn shake(&mut self) {
        use DeltaFSNode::*;

        let DeltaFSTree(tree) = self;

        for entry in tree.values_mut() {
            match entry {
                Leaf(Some(FSNode::Dir(m0, _, subtree0)), Some(FSNode::Dir(m1, _, subtree1))) => {
                    let optm = match m0.ne(&m1) {
                        true => Some((m0.clone(), m1.clone())),
                        false => None,
                    };
                    *entry = Branch(optm, get_delta(subtree0, subtree1));
                }
                Branch(optm, subdelta) => {
                    if let Some((old, new)) = optm {
                        if old == new {
                            *optm = None;
                        }
                    }
                    subdelta.shake()
                }
                _ => {}
            }
        }
        tree.retain(|_, child| match child {
            Leaf(pre, post) => pre != post,
            Branch(metadata, subdelta) => metadata.is_some() || (!subdelta.is_empty()),
        });
    }
    pub fn is_shaken(&self) -> bool {
        use DeltaFSNode::*;

        let DeltaFSTree(tree) = self;

        for entry in tree.values() {
            match entry {
                Leaf(pre, post) if pre == post => {
                    return false;
                }
                Leaf(Some(FSNode::Dir(_, _, _)), Some(FSNode::Dir(_, _, _))) => {
                    return false;
                }
                Branch(optm, subdelta) => {
                    if !subdelta.is_shaken() || (optm.is_none() && subdelta.is_empty()) {
                        return false;
                    }
                }
                _ => {}
            }
        }

        true
    }

    // TODO maybe these should return something about what they have filtered out?
    pub fn filter_out(&mut self, exclude_list: &ExcludeList) {
        self.filter_out_rec(&PathBuf::from("."), exclude_list);
    }
    fn filter_out_rec(&mut self, rel_path: &PathBuf, exclude_list: &ExcludeList) {
        let DeltaFSTree(tree) = self;
        for (name, child) in tree {
            match child {
                DeltaFSNode::Leaf(pre, post) => {
                    //--- PRE ---//
                    // TODO change this when let statement aren't unstable anymore
                    //	outside of if
                    let is_about_dir = if let Some(FSNode::Dir(_, _, _)) = pre {
                        true
                    } else {
                        false
                    };

                    if exclude_list.should_exclude(rel_path.join(name), is_about_dir) {
                        *pre = None;
                    }
                    //--- ---//

                    //--- POST ---//
                    // TODO change this when let statement aren't unstable anymore
                    //	outside of if
                    let is_about_dir = if let Some(FSNode::Dir(_, _, _)) = post {
                        true
                    } else {
                        false
                    };

                    if exclude_list.should_exclude(rel_path.join(name), is_about_dir) {
                        *post = None;
                    }
                    //--- ---//
                }
                DeltaFSNode::Branch(optm, subdelta) => {
                    if exclude_list.should_exclude(rel_path.join(name), true) {
                        *optm = None;
                        *subdelta = DeltaFSTree::empty();
                    } else {
                        subdelta.filter_out_rec(&rel_path.join(name), exclude_list);
                    }
                }
            }
        }
        self.shake();
    }

    pub fn merge_prec(&mut self, DeltaFSTree(prec): &DeltaFSTree) -> Result<(), UnmergeableDelta> {
        use std::collections::hash_map::Entry::*;
        use DeltaFSNode::*;

        let DeltaFSTree(succ) = self;
        for (name, child_prec) in prec {
            match succ.entry(name.clone()) {
                Vacant(entry) => {
                    entry.insert(child_prec.clone());
                }
                Occupied(mut entry) => match (child_prec, entry.get_mut()) {
                    (Branch(optm0, subdelta0), Branch(optm1, subdelta1)) => {
                        *optm1 = match (optm0.clone(), optm1.clone()) {
                            (None, None) => None,
                            (None, Some((old, new))) => Some((old, new)),
                            (Some((old, new)), None) => Some((old, new)),
                            (Some((old0, new0)), Some((old1, new1))) => {
                                if new0 == old1 {
                                    Some((old0, new1))
                                } else {
                                    return Err(unmergerr(name, "new metadata of precedent delta does not match with old metadata of successive delta"));
                                }
                            }
                        };
                        subdelta1.merge_prec(subdelta0).map_err(push_unmerg(name))?;
                    }
                    (Leaf(pre0, post0), Leaf(pre1, _)) => {
                        if post0 == pre1 {
                            *pre1 = pre0.clone();
                        } else {
                            return Err(unmergerr(name, "post state of precedent delta does not match with pre state of successive delta"));
                        }
                    }
                    (Leaf(pre0, post0), Branch(optm1, subdelta1)) => match post0 {
                        Some(FSNode::Dir(metadata, _, subtree)) => {
                            let mut subtree = subtree.clone();
                            subtree = subtree
								.try_apply_delta(subdelta1)
								.map_err(|_| unmergerr(name, "failed to apply subdelta of successive delta branch to precedent delta's directory leaf"))?;
                            let hash = hash_tree(&subtree);
                            let metadata = match optm1 {
                                Some((oldm, newm)) if metadata == oldm => newm.clone(),
                                _ => {
                                    return Err(unmergerr(name, "new metadata of precedent delta does not match with metadata of successive delta"));
                                }
                            };
                            entry.insert(Leaf(
                                pre0.clone(),
                                Some(FSNode::Dir(metadata, hash, subtree)),
                            ));
                        }
                        _ => {
                            return Err(unmergerr(name, "cannot merge branch delta (successive) with non dir leaf (precedent)"));
                        }
                    },
                    (Branch(optm0, subdelta0), Leaf(pre1, _)) => match pre1 {
                        Some(FSNode::Dir(metadata, hash, subtree)) => {
                            *subtree = subtree.try_undo_delta(subdelta0).map_err(|_| unmergerr(name, "failed to undo subdelta of precedent delta branch to successive delta's directory leaf"))?;
                            *hash = hash_tree(subtree);
                            if let Some((old, new)) = optm0 {
                                if new == metadata {
                                    *metadata = old.clone();
                                } else {
                                    return Err(unmergerr(name, "new metadata of precedent delta does not match with metadata of successive delta"));
                                }
                            }
                        }
                        _ => {
                            return Err(unmergerr(name, "cannot merge non dir leaf (successive) with branch delta (successive)"));
                        }
                    },
                },
            }
        }
        self.shake();
        Ok(())
    }
}

pub fn get_delta(FSTree(old_tree): &FSTree, FSTree(new_tree): &FSTree) -> DeltaFSTree {
    use FSNode::*;
    let mut delta: HashMap<String, DeltaFSNode> = HashMap::new();

    for (key, ior) in union(old_tree, new_tree) {
        match ior {
            IOr::Left(child0) => {
                delta.insert(key, DeltaFSNode::remove(child0));
            }
            IOr::Right(child1) => {
                delta.insert(key, DeltaFSNode::add(child1));
            }
            IOr::Both(child0, child1) => {
                if let (Dir(m0, h0, subtree0), Dir(m1, h1, subtree1)) = (child0, child1) {
                    if m0.ne(m1) || h0.ne(h1) {
                        let delta_metadata = match m0.ne(m1) {
                            true => Some((m0.clone(), m1.clone())),
                            false => None,
                        };
                        let delta_subtree = match h0.ne(h1) {
                            true => get_delta(subtree0, subtree1),
                            false => DeltaFSTree::empty(),
                        };
                        delta.insert(key, DeltaFSNode::Branch(delta_metadata, delta_subtree));
                    }
                } else {
                    if let Some(node) = DeltaFSNode::edit(child0, child1) {
                        delta.insert(key, node);
                    }
                }
            }
        }
    }

    DeltaFSTree(delta)
}

impl FSTree {
    pub fn try_apply_delta(
        &self,
        DeltaFSTree(deltatree): &DeltaFSTree,
    ) -> Result<FSTree, InapplicableDelta> {
        use std::collections::hash_map::Entry::*;
        use DeltaFSNode::*;
        let FSTree(mut fstree) = self.clone();
        for (name, child) in deltatree {
            match child {
                Leaf(None, None) => {
                    println!(
                        "The delta was not shaken! Instance of Leaf(None, None), at {}",
                        name
                    );
                    match fstree.entry(name.clone()) {
                        Vacant(_) => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                name,
                                "delta claims this node is None, but it exists in tree",
                            ));
                        }
                    }
                }
                Leaf(Some(pre), Some(post)) if pre == post => {
                    println!("The delta was not shaken! Instance of Leaf(Some(pre), Some(post)) with pre == post, at {}", name);
                    match fstree.entry(name.clone()) {
                        Occupied(entry) if entry.get() == pre => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                name,
                                "delta pre state for this node does not match with node in tree",
                            ));
                        }
                        Vacant(_) => {
                            return Err(inapperr(
                                name,
                                "delta claims this node is Some, but it does not exist in tree",
                            ));
                        }
                    }
                }
                Leaf(Some(pre), None) => match fstree.entry(name.clone()) {
                    Occupied(entry) if entry.get() == pre => {
                        entry.remove();
                    }
                    Occupied(_) => {
                        return Err(inapperr(
                            name,
                            "delta pre state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is Some, but it does not exist in tree",
                        ));
                    }
                },
                Leaf(None, Some(post)) => match fstree.entry(name.clone()) {
                    Vacant(entry) => {
                        entry.insert(post.clone());
                    }
                    Occupied(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is None, but it exists in tree",
                        ));
                    }
                },
                Leaf(Some(pre), Some(post)) => match fstree.entry(name.clone()) {
                    Occupied(mut entry) if entry.get() == pre => {
                        entry.insert(post.clone());
                    }
                    Occupied(_) => {
                        return Err(inapperr(
                            name,
                            "delta pre state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is Some, but it does not exist in tree",
                        ));
                    }
                },
                Branch(optm, subdelta) => match fstree.entry(name.clone()) {
                    Occupied(mut entry) => match entry.get_mut() {
                        FSNode::Dir(metadata, hash, subtree) => {
                            if let Some((old, new)) = optm {
                                if metadata.eq(&old) {
                                    *metadata = new.clone();
                                } else {
                                    return Err(inapperr(name, "metadata of directory does not match old metadata of delta branch"));
                                }
                            }
                            *subtree = subtree
                                .try_apply_delta(subdelta)
                                .map_err(push_inapp(name))?;
                            *hash = hash_tree(subtree);
                        }
                        FSNode::File(_, _) => {
                            return Err(inapperr(
                                name,
                                "delta claims this node is a directory, but it is a file in tree",
                            ));
                        }
                        FSNode::SymLink(_) => {
                            return Err(inapperr(
								name,
								"delta claims this node is a directory, but it is a symlink in tree",
							));
                        }
                    },
                    Vacant(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is a directory, but it does not exist in tree",
                        ));
                    }
                },
            }
        }
        Ok(FSTree(fstree))
    }

    pub fn try_undo_delta(
        &self,
        DeltaFSTree(deltatree): &DeltaFSTree,
    ) -> Result<FSTree, InapplicableDelta> {
        use std::collections::hash_map::Entry::*;
        use DeltaFSNode::*;
        let FSTree(mut fstree) = self.clone();
        for (name, child) in deltatree {
            match child {
                Leaf(None, None) => {
                    println!(
                        "The delta was not shaken! Instance of Leaf(None, None), at {}",
                        name
                    );
                    match fstree.entry(name.clone()) {
                        Vacant(_) => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                name,
                                "delta claims this node is None, but it exists in tree",
                            ));
                        }
                    }
                }
                Leaf(Some(pre), Some(post)) if pre == post => {
                    println!("The delta was not shaken! Instance of Leaf(Some(pre), Some(post)) with pre == post, at {}", name);
                    match fstree.entry(name.clone()) {
                        Occupied(entry) if entry.get() == post => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                name,
                                "delta post state for this node does not match with node in tree",
                            ));
                        }
                        Vacant(_) => {
                            return Err(inapperr(
                                name,
                                "delta claims this node is Some, but it does not exist in tree",
                            ));
                        }
                    }
                }
                Leaf(Some(pre), None) => match fstree.entry(name.clone()) {
                    Vacant(entry) => {
                        entry.insert(pre.clone());
                    }
                    Occupied(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is None, but it exists in tree",
                        ));
                    }
                },
                Leaf(None, Some(post)) => match fstree.entry(name.clone()) {
                    Occupied(entry) if entry.get() == post => {
                        entry.remove();
                    }
                    Occupied(_) => {
                        return Err(inapperr(
                            name,
                            "delta post state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is Some, but it does not exist in tree",
                        ));
                    }
                },
                Leaf(Some(pre), Some(post)) => match fstree.entry(name.clone()) {
                    Occupied(mut entry) if entry.get() == post => {
                        entry.insert(pre.clone());
                    }
                    Occupied(_) => {
                        return Err(inapperr(
                            name,
                            "delta post state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is Some, but it does not exist in tree",
                        ));
                    }
                },
                Branch(optm, subdelta) => match fstree.entry(name.clone()) {
                    Occupied(mut entry) => match entry.get_mut() {
                        FSNode::Dir(metadata, hash, subtree) => {
                            if let Some((old, new)) = optm {
                                if metadata.eq(&new) {
                                    *metadata = old.clone();
                                } else {
                                    return Err(inapperr(name, "metadata of directory does not match new metadata of delta branch"));
                                }
                            }
                            *subtree =
                                subtree.try_undo_delta(subdelta).map_err(push_inapp(name))?;
                            *hash = hash_tree(subtree);
                        }
                        FSNode::File(_, _) => {
                            return Err(inapperr(
                                name,
                                "delta claims this node is a directory, but it is a file in tree",
                            ));
                        }
                        FSNode::SymLink(_) => {
                            return Err(inapperr(
								name,
								"delta claims this node is a directory, but it is a symlink in tree",
							));
                        }
                    },
                    Vacant(_) => {
                        return Err(inapperr(
                            name,
                            "delta claims this node is a directory, but it does not exist in tree",
                        ));
                    }
                },
            }
        }
        Ok(FSTree(fstree))
    }
}
