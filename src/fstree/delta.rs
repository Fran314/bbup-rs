use super::{hash_tree, FSNode, FSTree};
use crate::fs::AbstPath;
use crate::ior::{union, IOr};

use crate::{fs::Mtime, model::ExcludeList};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeltaError {
    #[error("File System Tree Delta Error: inner error occurred\nSource: {src}\nError: {err}")]
    InnerError { src: String, err: String },

    #[error("File System Tree Delta Error: some error occurred.\nSource: {src}\nError: {err}")]
    GenericError { src: String, err: String },
}

#[derive(Error, Debug)]
#[error(
    "File System Tree Delta Error: unable to apply delta to tree.\nConflict at path: {0}\nError: {1}"
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
#[error("File System Tree Delta Error: unable to merge deltas.\nConflict at path: {0}\nError: {1}")]
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
pub enum DeltaNode {
    Leaf(Option<FSNode>, Option<FSNode>),
    Branch(Option<(Mtime, Mtime)>, Delta),
}
impl DeltaNode {
    fn remove(pre: &FSNode) -> DeltaNode {
        DeltaNode::Leaf(Some(pre.clone()), None)
    }
    fn add(post: &FSNode) -> DeltaNode {
        DeltaNode::Leaf(None, Some(post.clone()))
    }
    fn edit(pre: &FSNode, post: &FSNode) -> DeltaNode {
        DeltaNode::Leaf(Some(pre.clone()), Some(post.clone()))
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Delta(pub HashMap<String, DeltaNode>);
impl Delta {
    pub fn empty() -> Delta {
        Delta(HashMap::new())
    }
    pub fn is_empty(&self) -> bool {
        let Delta(hashmap) = self;
        hashmap.len() == 0
    }

    /// Given a delta (self) and a path of a possible subtree, tries to get what
    /// the delta changed on the subtree at the specified path, assuming that
    /// the specified path is an actual subtree and not just a leaf.
    ///
    /// This results in:
    /// - the total delta, if the path is empty (hence the "subtree" was the
    ///     whole tree)
    /// - nothing, if the subtree was untouched by the delta (ie if the
    ///     subdelta for the specified subtree doesn't exist)
    /// - nothing, if the specified subtree was actually a leaf
    /// - the subdelta, translated in a subdeltatree if it was a subdeltaleaf
    ///
    /// This function assumes that the given delta is shaken, and will not work
    /// as expected otherwise
    pub fn get_subdelta_tree_copy(&self, path: &AbstPath) -> Option<Delta> {
        match path.get(0) {
            None => Some(self.clone()),
            Some(name) => {
                let Delta(tree) = self;
                match tree.get(name) {
                    None => None,
                    Some(DeltaNode::Branch(_, subdelta)) => {
                        subdelta.get_subdelta_tree_copy(&path.strip_first())
                    }
                    Some(DeltaNode::Leaf(None, Some(FSNode::Dir(_, _, FSTree(subtree))))) => {
                        let mut subdelta: HashMap<String, DeltaNode> = HashMap::new();
                        for (node, child) in subtree {
                            subdelta
                                .insert(node.clone(), DeltaNode::Leaf(None, Some(child.clone())));
                        }
                        Delta(subdelta).get_subdelta_tree_copy(&path.strip_first())
                    }
                    Some(DeltaNode::Leaf(Some(FSNode::Dir(_, _, FSTree(subtree))), None)) => {
                        let mut subdelta: HashMap<String, DeltaNode> = HashMap::new();
                        for (node, child) in subtree {
                            subdelta
                                .insert(node.clone(), DeltaNode::Leaf(Some(child.clone()), None));
                        }
                        Delta(subdelta).get_subdelta_tree_copy(&path.strip_first())
                    }

                    // I think this assumes that the delta is shaken
                    Some(DeltaNode::Leaf(_, _)) => None,
                }
            }
        }
    }
    pub fn shake(&mut self) {
        use DeltaNode::*;

        let Delta(tree) = self;

        for entry in tree.values_mut() {
            match entry {
                Leaf(Some(FSNode::Dir(m0, _, subtree0)), Some(FSNode::Dir(m1, _, subtree1))) => {
                    let optm = match m0 != m1 {
                        true => Some((m0.clone(), m1.clone())),
                        false => None,
                    };
                    *entry = Branch(optm, get_delta(subtree0, subtree1));
                }
                Branch(_, subdelta) => subdelta.shake(),
                _ => {}
            }
        }
        tree.retain(|_, child| match child {
            Leaf(pre, post) => pre != post,
            Branch(optm, subdelta) => optm.is_some() || (!subdelta.is_empty()),
        });
    }
    pub fn is_shaken(&self) -> bool {
        use DeltaNode::*;

        let Delta(tree) = self;

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
        self.filter_out_rec(&AbstPath::single("."), exclude_list);
    }
    fn filter_out_rec(&mut self, rel_path: &AbstPath, exclude_list: &ExcludeList) {
        let Delta(tree) = self;
        for (name, child) in tree {
            match child {
                DeltaNode::Leaf(pre, post) => {
                    //--- PRE ---//
                    let is_about_dir = matches!(pre, Some(FSNode::Dir(_, _, _)));
                    if exclude_list.should_exclude(&rel_path.add_last(name), is_about_dir) {
                        *pre = None;
                    }
                    //--- ---//

                    //--- POST ---//
                    let is_about_dir = matches!(post, Some(FSNode::Dir(_, _, _)));
                    if exclude_list.should_exclude(&rel_path.add_last(name), is_about_dir) {
                        *post = None;
                    }
                    //--- ---//
                }
                DeltaNode::Branch(optm, subdelta) => {
                    if exclude_list.should_exclude(&rel_path.add_last(name), true) {
                        // Make it so that the branch will be removed once the
                        //	delta gets shaken at the end of the function
                        *optm = None;
                        *subdelta = Delta::empty();
                    } else {
                        subdelta.filter_out_rec(&rel_path.add_last(name), exclude_list);
                    }
                }
            }
        }
        self.shake();
    }

    pub fn merge_prec(&mut self, Delta(prec): &Delta) -> Result<(), UnmergeableDelta> {
        use std::collections::hash_map::Entry::*;
        use DeltaNode::*;

        let Delta(succ) = self;
        for (name, child_prec) in prec {
            match succ.entry(name.clone()) {
                Vacant(entry) => {
                    entry.insert(child_prec.clone());
                }
                Occupied(mut entry) => match (child_prec, entry.get_mut()) {
                    (Branch(optm0, subdelta0), Branch(optm1, subdelta1)) => {
                        let optm = match (optm0.clone(), optm1.clone()) {
                            (Some((premtime0, postmtime0)), Some((premtime1, postmtime1))) => {
                                if postmtime0 != premtime1 {
                                    return Err(unmergerr(name, "new mtime of precedent delta does not match with old mtime of successive delta"));
                                } else {
                                    Some((premtime0, postmtime1))
                                }
                            }
                            (Some((premtime0, postmtime0)), None) => Some((premtime0, postmtime0)),
                            (None, Some((premtime1, postmtime1))) => Some((premtime1, postmtime1)),
                            (None, None) => None,
                        };
                        *optm1 = optm;
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
                        Some(FSNode::Dir(mtime, _, subtree)) => {
                            let mut subtree = subtree.clone();
                            subtree
								.apply_delta(subdelta1)
								.map_err(|_| unmergerr(name, "failed to apply subdelta of successive delta branch to precedent delta's directory leaf"))?;
                            let mtime = match optm1 {
                                Some((premtime1, postmtime1)) => {
                                    if mtime != premtime1 {
                                        return Err(unmergerr(name, "new mtime of precedent delta does not match with mtime of successive delta"));
                                    }
                                    postmtime1.clone()
                                }
                                None => mtime.clone(),
                            };
                            let hash = hash_tree(&subtree);
                            entry.insert(Leaf(
                                pre0.clone(),
                                Some(FSNode::Dir(mtime, hash, subtree)),
                            ));
                        }
                        _ => {
                            return Err(unmergerr(name, "cannot merge branch delta (successive) with non dir leaf (precedent)"));
                        }
                    },
                    (Branch(optm0, subdelta0), Leaf(pre1, _)) => match pre1 {
                        Some(FSNode::Dir(mtime, hash, subtree)) => {
                            subtree.undo_delta(subdelta0).map_err(|_| unmergerr(name, "failed to undo subdelta of precedent delta branch to successive delta's directory leaf"))?;
                            *hash = hash_tree(subtree);
                            match optm0 {
                                Some((premtime0, postmtime0)) => {
                                    if postmtime0 != mtime {
                                        return Err(unmergerr(name, "new metadata of precedent delta does not match with metadata of successive delta"));
                                    } else {
                                        *mtime = premtime0.clone();
                                    }
                                }
                                None => {
                                    // Leave mtime unchanged because the previous
                                    //	delta doesn't change the mtime so the
                                    //	premtime is the same
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

pub fn get_delta(FSTree(last_known_fstree): &FSTree, FSTree(new_tree): &FSTree) -> Delta {
    use FSNode::*;
    let mut delta: HashMap<String, DeltaNode> = HashMap::new();

    for (key, ior) in union(last_known_fstree, new_tree) {
        match ior {
            IOr::Left(child0) => {
                delta.insert(key, DeltaNode::remove(child0));
            }
            IOr::Right(child1) => {
                delta.insert(key, DeltaNode::add(child1));
            }
            IOr::Both(child0, child1) => {
                if let (Dir(m0, h0, subtree0), Dir(m1, h1, subtree1)) = (child0, child1) {
                    if m0.ne(m1) || h0.ne(h1) {
                        let delta_mtime = match m0.ne(m1) {
                            true => Some((m0.clone(), m1.clone())),
                            false => None,
                        };
                        let delta_subtree = match h0.ne(h1) {
                            true => get_delta(subtree0, subtree1),
                            false => Delta::empty(),
                        };
                        delta.insert(key, DeltaNode::Branch(delta_mtime, delta_subtree));
                    }
                } else if child0 != child1 {
                    delta.insert(key, DeltaNode::edit(child0, child1));
                }
            }
        }
    }

    Delta(delta)
}

impl FSTree {
    pub fn apply_delta_at_endpoint(
        &mut self,
        delta: &Delta,
        endpoint: AbstPath,
    ) -> Result<(), InapplicableDelta> {
        match endpoint.get(0) {
            None => self.apply_delta(delta),
            Some(name) => {
                let FSTree(fstree) = self;
                match fstree.get_mut(name) {
                    Some(FSNode::Dir(_, _, subtree)) => {
                        subtree.apply_delta_at_endpoint(delta, endpoint.strip_first())
                    }
                    Some(FSNode::File(_, _)) => Err(inapperr(
                        name,
                        "endpoint claims this node is a directory, but it is a file",
                    )),
                    Some(FSNode::SymLink(_, _)) => Err(inapperr(
                        name,
                        "endpoint claims this node is a directory, but it is a symlink",
                    )),
                    None => Err(inapperr(
                        name,
                        "endpoint claims this node is a directory, but it doesn't exist",
                    )),
                }
            }
        }
    }
    pub fn apply_delta(&mut self, Delta(deltatree): &Delta) -> Result<(), InapplicableDelta> {
        use std::collections::hash_map::Entry::{Occupied, Vacant};
        use DeltaNode::{Branch, Leaf};
        let FSTree(fstree) = self;
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
                        FSNode::Dir(mtime, hash, subtree) => {
                            match optm {
                                Some((premtime, postmtime)) => {
                                    if mtime != premtime {
                                        return Err(inapperr(name, "mtime of directory does not match old mtime of delta branch"));
                                    }
                                    *mtime = postmtime.clone();
                                }
                                None => {
                                    // Leave the mtime unchanged
                                }
                            }
                            subtree.apply_delta(subdelta).map_err(push_inapp(name))?;
                            *hash = hash_tree(subtree);
                        }
                        FSNode::File(_, _) => {
                            return Err(inapperr(
                                name,
                                "delta claims this node is a directory, but it is a file in tree",
                            ));
                        }
                        FSNode::SymLink(_, _) => {
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
        Ok(())
    }
    pub fn undo_delta(&mut self, Delta(deltatree): &Delta) -> Result<(), InapplicableDelta> {
        use std::collections::hash_map::Entry::*;
        use DeltaNode::*;
        let FSTree(fstree) = self;
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
                Branch(optm, subdelta) => {
                    match fstree.entry(name.clone()) {
                        Occupied(mut entry) => match entry.get_mut() {
                            FSNode::Dir(mtime, hash, subtree) => {
                                match optm {
                                    Some((premtime, postmtime)) => {
                                        if mtime != postmtime {
                                            return Err(inapperr(name, "mtime of directory does not match new mtime of delta branch"));
                                        }
                                        *mtime = premtime.clone();
                                    }
                                    None => {
                                        // Leave mtime unchanged
                                    }
                                }
                                subtree.apply_delta(subdelta).map_err(push_inapp(name))?;
                                // *subtree =
                                //     subtree.try_undo_delta(subdelta).map_err(push_inapp(name))?;
                                *hash = hash_tree(subtree);
                            }
                            FSNode::File(_, _) => {
                                return Err(inapperr(
                                name,
                                "delta claims this node is a directory, but it is a file in tree",
                            ));
                            }
                            FSNode::SymLink(_, _) => {
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
                    }
                }
            }
        }
        Ok(())
    }
}
