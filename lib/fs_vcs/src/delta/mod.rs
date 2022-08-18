use super::{hash_tree, ExcludeList, FSNode, FSTree};
use abst_fs::{AbstPath, Mtime};
use ior::{union, IOr};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod apply;
mod merge;

pub use merge::UnmergeableDelta;

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

    // TODO maybe these should return something about what they have filtered out?
    pub fn filter_out(&mut self, exclude_list: &ExcludeList) {
        self.filter_out_rec(&AbstPath::single("."), exclude_list);
    }
    fn filter_out_rec(&mut self, rel_path: &AbstPath, exclude_list: &ExcludeList) {
        let Delta(tree) = self;
        for (name, child) in tree {
            match child {
                DeltaNode::Leaf(pre, post) => {
                    if exclude_list.should_exclude(
                        &rel_path.add_last(name),
                        matches!(pre, Some(FSNode::Dir(_, _, _))),
                    ) {
                        *pre = None;
                    }

                    if exclude_list.should_exclude(
                        &rel_path.add_last(name),
                        matches!(post, Some(FSNode::Dir(_, _, _))),
                    ) {
                        *post = None;
                    }
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
