use std::collections::HashMap;

use abst_fs::{Endpoint, Mtime};
use hasher::Hash;

use fs_vcs::{FSNode, FSTree};

#[derive(Clone)]
pub enum PhmFSNode {
    File(Mtime, Hash),
    SymLink(Mtime, Endpoint),
    Dir(Mtime, Hash, PhmFSTree),
    Phantom(FSNode),
}
impl PartialEq for PhmFSNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(mtime_l, hash_l), Self::File(mtime_r, hash_r))

            // Do not check for subtree structure: the idea is that the hash represents
            //	itself the tree structure, so the trees are equal iff the hashes are
            //	equal, hence check the hash and not the subtree
            | (Self::Dir(mtime_l, hash_l, _), Self::Dir(mtime_r, hash_r, _)) => {
                mtime_l == mtime_r && hash_l == hash_r
            }

            (Self::SymLink(mtime_l, endpoint_l), Self::SymLink(mtime_r, endpoint_r)) => {
                mtime_l == mtime_r && endpoint_l == endpoint_r
            }

            (Self::Phantom(node_l), Self::Phantom(node_r)) => {
                node_l == node_r
            }

            _ => false,
        }
    }
}
impl From<FSNode> for PhmFSNode {
    fn from(value: FSNode) -> Self {
        match value {
            FSNode::File(mtime, hash) => PhmFSNode::File(mtime, hash),
            FSNode::SymLink(mtime, endpoint) => PhmFSNode::SymLink(mtime, endpoint),
            FSNode::Dir(mtime, hash, subtree) => PhmFSNode::Dir(mtime, hash, subtree.into()),
        }
    }
}
impl PhmFSNode {
    /// Convert PhmFSNode to FSNode
    pub fn unphantom(&self) -> FSNode {
        match self {
            PhmFSNode::Phantom(node) => node.clone(),
            PhmFSNode::File(mtime, hash) => FSNode::File(mtime.clone(), hash.clone()),
            PhmFSNode::SymLink(mtime, endpoint) => FSNode::SymLink(mtime.clone(), endpoint.clone()),
            PhmFSNode::Dir(mtime, hash, subtree) => {
                FSNode::Dir(mtime.clone(), hash.clone(), subtree.unphantom())
            }
        }
    }

    pub fn hash_node(&self) -> Hash {
        use hasher::hash_bytes;
        let mut s: Vec<u8> = Vec::new();
        match self {
            PhmFSNode::File(mtime, hash) => {
                s.append(&mut b"f".to_vec());
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            PhmFSNode::SymLink(mtime, endpoint) => {
                s.append(&mut b"s".to_vec());
                s.append(&mut mtime.to_bytes());
                // As for the name in `hash_tree`, we add the hash of the endpoint
                // and not the endpoint itself as bytes to avoid unlikely but
                // possible collisions.
                s.append(&mut hash_bytes(endpoint.as_bytes()).to_bytes());
            }
            PhmFSNode::Dir(mtime, hash, _) => {
                s.append(&mut b"d".to_vec());
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            PhmFSNode::Phantom(node) => {
                s.append(&mut b"p".to_vec());
                s.append(&mut node.hash_node().to_bytes());
            }
        }
        hash_bytes(s)
    }
}

#[derive(Clone)]
pub struct PhmFSTree(HashMap<String, PhmFSNode>);
impl From<FSTree> for PhmFSTree {
    fn from(value: FSTree) -> Self {
        let mut output = PhmFSTree::new();
        for (name, child) in value {
            output.insert(name, child.into());
        }
        output
    }
}

/// IntoIterator implementation for PhmFSTree
/// Note: despite PhmFSTree being a wrapper for an hashmap which usually
/// iterates on its content in random order, PhmFSTree is guaranteed to be
/// iterated alphabetically
impl IntoIterator for PhmFSTree {
    type Item = (String, PhmFSNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.into_iter().collect::<Vec<(String, PhmFSNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}
/// IntoIterator implementation for &PhmFSTree
/// Note: despite PhmFSTree being a wrapper for an hashmap which usually
/// iterates on its content in random order, PhmFSTree is guaranteed to be
/// iterated alphabetically
impl<'a> IntoIterator for &'a PhmFSTree {
    type Item = (&'a String, &'a PhmFSNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.iter().collect::<Vec<(&String, &PhmFSNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}

#[allow(clippy::new_without_default)]
impl PhmFSTree {
    pub fn inner(&self) -> &HashMap<String, PhmFSNode> {
        &self.0
    }

    pub fn new() -> PhmFSTree {
        PhmFSTree(HashMap::new())
    }

    pub fn insert(&mut self, name: impl ToString, child: PhmFSNode) -> Option<PhmFSNode> {
        self.0.insert(name.to_string(), child)
    }

    // pub fn get(&self, name: impl ToString) -> Option<&FSNode> {
    //     self.0.get(&name.to_string())
    // }
    //
    // pub fn retain(&mut self, filter: impl FnMut(&String, &mut FSNode) -> bool) {
    //     self.0.retain(filter)
    // }
    //
    // pub fn entry(&mut self, e: String) -> std::collections::hash_map::Entry<String, FSNode> {
    //     self.0.entry(e)
    // }

    /// Convert PhmFSTree to FSTree
    /// Any phantom node will be translated into the non-phantom FSNode version
    /// of itself.
    /// The reason behind this is that this function is used specifically to
    /// translate a non-phantom directory node to a phantom node containing
    /// said directory, so everything (including the directory) will become
    /// phantom by being inside the phantom directory node, and nodes that were
    /// already phantom will be practically uneffected
    pub fn unphantom(&self) -> FSTree {
        let mut output = FSTree::new();

        for (name, child) in self {
            output.insert(name, child.unphantom());
        }

        output
    }

    pub fn hash_tree(&self) -> Hash {
        use hasher::hash_bytes;
        let mut s: Vec<u8> = Vec::new();
        for (name, node) in self {
            // The reason why we append the hash of the name and not the name itself
            //	is to avoid unlikely but possible collisions.
            // This makes the appended blocks all the same length, which is better
            let name_hash = hash_bytes(name.as_bytes());
            s.append(&mut name_hash.to_bytes());
            s.append(&mut node.hash_node().to_bytes());
        }
        hash_bytes(s)
    }
}
