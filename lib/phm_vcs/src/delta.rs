use std::collections::HashMap;

use abst_fs::Mtime;
use fs_vcs::{Delta, DeltaNode, FSNode, FSTree};

use crate::{PhmFSNode, PhmFSTree};

use ior::{union, IOr};

/// Nodes of a delta between a phantom fstree and a normal fstree
///
/// This delta is a bit weird in the sense that it's fairly different from the
/// Delta between to fstrees. This is because this type of delta serves a much
/// simpler purpose.
/// The only thing that this delta has to do is to update the state of a local
/// injective backup source, it will only exist locally and will never be
/// merged or reversed.
/// This is the reason why there is no "Remove" type and why the "Vanish"
/// (which represents a node going from existing to phantom) does not contain
/// any info on what vanished: it's not possible for a node in one local update
/// to go from an existing state to a phantom state with different internal
/// state (as in, if an object is a ::File, it can become ::Phantom(::File) in
/// one local update but it cant become a ::Phantom(::Dir) in one local update)
/// That is also why Add takes a FSNode (you can't "add" a phantom (as in non
/// existing) node because if you added it locally then it exists locally) and
/// same goes for Edit which takes a FSNode as second parameter
///
/// It's not the cleanest solution ever but it ensures that only meaningful
/// data can be represented
pub enum PhmDeltaNode {
    Vanish,
    Add(FSNode),
    Edit(PhmFSNode, FSNode),

    Branch((Mtime, Mtime), PhmDelta),
}
impl PhmDeltaNode {
    pub fn to_positive_deltanode(&self) -> Option<DeltaNode> {
        match self {
            PhmDeltaNode::Vanish => None,
            PhmDeltaNode::Add(child1) => Some(DeltaNode::Leaf(None, Some(child1.clone()))),
            PhmDeltaNode::Edit(child0, child1) => match child0 {
                // If the node was phantom, then further inspection is needed
                PhmFSNode::Phantom(inner0) => {
                    if inner0 == child1 {
                        // if inner0 == child1 then what happened is that the
                        // node "resurrected" (ie it went from phantom to
                        // existing, but remained the same) so from the server
                        // point of view, where objects always exist, nothing
                        // changed on this node, and no change is needed
                        None
                    } else {
                        // Otherwise, it was added as something else, which
                        // means that from the server point of view the object
                        // actually changed and a delta is needed
                        Some(DeltaNode::Leaf(Some(inner0.clone()), Some(child1.clone())))
                    }
                }

                // Otherwise, this was a regular object that was edited to a
                // (hopefully different) regular object, which means a leaf
                // delta is needed
                _ => Some(DeltaNode::Leaf(
                    Some(child0.unphantom()),
                    Some(child1.clone()),
                )),
            },
            PhmDeltaNode::Branch((m0, m1), subdelta) => {
                let subpositive = subdelta.to_positive_delta();
                if m0.ne(m1) || !subpositive.is_empty() {
                    Some(DeltaNode::Branch((m0.clone(), m1.clone()), subpositive))
                } else {
                    None
                }
            }
        }
    }
}

pub struct PhmDelta(HashMap<String, PhmDeltaNode>);

/// IntoIterator implementation for PhmDelta
/// Note: despite PhmDelta being a wrapper for an hashmap which usually
/// iterates on its content in random order, PhmDelta is guaranteed to be
/// iterated alphabetically
impl IntoIterator for PhmDelta {
    type Item = (String, PhmDeltaNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.into_iter().collect::<Vec<(String, PhmDeltaNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}
/// IntoIterator implementation for &PhmDelta
/// Note: despite PhmDelta being a wrapper for an hashmap which usually
/// iterates on its content in random order, PhmDelta is guaranteed to be
/// iterated alphabetically
impl<'a> IntoIterator for &'a PhmDelta {
    type Item = (&'a String, &'a PhmDeltaNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.iter().collect::<Vec<(&String, &PhmDeltaNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}
/// IntoIterator implementation for &mut PhmDelta
/// Note: despite PhmDelta being a wrapper for an hashmap which usually
/// iterates on its content in random order, PhmDelta is guaranteed to be
/// iterated alphabetically
impl<'a> IntoIterator for &'a mut PhmDelta {
    type Item = (&'a String, &'a mut PhmDeltaNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self
            .0
            .iter_mut()
            .collect::<Vec<(&String, &mut PhmDeltaNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}

#[allow(clippy::new_without_default)]
impl PhmDelta {
    pub fn new() -> PhmDelta {
        PhmDelta(HashMap::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    pub fn insert(&mut self, name: impl ToString, child: PhmDeltaNode) -> Option<PhmDeltaNode> {
        self.0.insert(name.to_string(), child)
    }
    // pub fn get(&self, name: impl ToString) -> Option<&PhmDeltaNode> {
    //     self.0.get(&name.to_string())
    // }
    // pub fn values_mut(&mut self) -> std::collections::hash_map::ValuesMut<String, PhmDeltaNode> {
    //     self.0.values_mut()
    // }
    // pub fn retain(&mut self, filter: impl FnMut(&String, &mut PhmDeltaNode) -> bool) {
    //     self.0.retain(filter)
    // }
    // pub fn entry(&mut self, e: String) -> std::collections::hash_map::Entry<String, PhmDeltaNode> {
    //     self.0.entry(e)
    // }
    //
    // pub fn shake(&mut self) {
    //     use PhmDeltaNode::*;
    //
    //     for entry in self.values_mut() {
    //         match entry {
    //             Leaf(Some(FSNode::Dir(m0, _, subtree0)), Some(FSNode::Dir(m1, _, subtree1))) => {
    //                 // We assume here that if a delta is generated with
    //                 // get_delta then it is automatically already shaken, and no
    //                 // recursion is needed
    //                 *entry = Branch((m0.clone(), m1.clone()), subtree0.get_delta_to(subtree1));
    //             }
    //             Branch(_, subdelta) => subdelta.shake(),
    //             Leaf(_, _) => {}
    //         }
    //     }
    //     self.retain(|_, child| match child {
    //         Leaf(pre, post) => pre != post,
    //         Branch((premtime, postmtime), subdelta) => {
    //             premtime != postmtime || (!subdelta.is_empty())
    //         }
    //     });
    // }

    pub fn to_positive_delta(&self) -> Delta {
        let mut output = Delta::new();

        for (name, child) in self {
            if let Some(delta) = child.to_positive_deltanode() {
                output.insert(name, delta);
            }
        }

        output
    }
}

impl PhmFSTree {
    pub fn get_delta_to(&self, new_tree: &FSTree) -> PhmDelta {
        let mut delta = PhmDelta::new();

        for (key, ior) in union(self.inner(), new_tree.inner()) {
            match ior {
                IOr::Left(child0) => {
                    match child0 {
                        // Phantom object is absent in the new fstree, which
                        // means it remained phantom. This means nothing
                        // changed in the phantom fstree and no delta is needed
                        PhmFSNode::Phantom(_) => {}

                        // Otherwise, an object that was present before is now
                        // absent, which means it became a phantom version of
                        // itself
                        _ => {
                            delta.insert(key, PhmDeltaNode::Vanish);
                        }
                    }
                }
                IOr::Right(child1) => {
                    delta.insert(key, PhmDeltaNode::Add(child1.clone()));
                }
                IOr::Both(child0, child1) => {
                    if let (PhmFSNode::Dir(m0, h0, subtree0), FSNode::Dir(m1, h1, subtree1)) =
                        (child0, child1)
                    {
                        let delta_subtree = match h0.ne(h1) {
                            true => subtree0.get_delta_to(subtree1),
                            false => PhmDelta::new(),
                        };

                        // Note: delta_subtree could be empty even if h0 != h1,
                        // like in the following case:
                        //  subtree0: [
                        //      "phantom-object" -> PhmFSNode::Phantom([anything])
                        //  ]
                        //  subtree1: [ ] // empty
                        //
                        //  The hashes of these two trees are different (mainly
                        //  is empty and the other isn't), but given that the
                        //  missing object in the FSTree is a phantom object in
                        //  the PhmFSTree, no delta is needed to update the
                        //  subtree0 because the phantom object remained
                        //  missing (ie: phantom)
                        if m0.ne(m1) || !delta_subtree.is_empty() {
                            let delta_subtree = match h0.ne(h1) {
                                true => subtree0.get_delta_to(subtree1),
                                false => PhmDelta::new(),
                            };
                            delta.insert(
                                key,
                                PhmDeltaNode::Branch((m0.clone(), m1.clone()), delta_subtree),
                            );
                        }
                    } else {
                        // The reason why we convert child1 to PhmFSNode to
                        // check equality and not child0 to FSNode is because
                        // if we convert child0 to FSNode we potentially lose
                        // information on whether it was phantom or not, and a
                        // Phantom(X) which becomes an X does require a change
                        // (ie Phantom(X) -> X.into::<PhmFSNode>()), which
                        // would be lost by that conversion
                        let phm_child1: PhmFSNode = child1.clone().into();
                        if child0 != &phm_child1 {
                            delta.insert(key, PhmDeltaNode::Edit(child0.clone(), child1.clone()));
                        }
                    }
                }
            }
        }
        delta
    }
}
