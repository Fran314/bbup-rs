use super::{hash_tree, Delta, DeltaNode, FSNode, FSTree};

use abst_fs::AbstPath;
use std::collections::HashMap;
use thiserror::Error;

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

impl Delta {
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
}
