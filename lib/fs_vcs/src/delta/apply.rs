use super::{hash_tree, Delta, DeltaNode, FSNode, FSTree};
use abst_fs::AbstPath;

use thiserror::Error;

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
                                subtree.undo_delta(subdelta).map_err(push_inapp(name))?;
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
