use std::collections::HashMap;

use crate::fs::Metadata;

use super::{intersect, DeltaFSNode, DeltaFSTree, FSNode, FSTree, IOr};

pub enum ConflictNode {
    Leaf(DeltaFSNode, DeltaFSNode),
    Branch(IOr<((Metadata, Metadata), (Metadata, Metadata)), ConflictTree>),
}
pub struct ConflictTree(pub HashMap<String, ConflictNode>);

fn compatilble_added_subtrees(FSTree(subtree0): &FSTree, FSTree(subtree1): &FSTree) -> bool {
    for (_, (left, right)) in intersect(subtree0, subtree1) {
        let compatible = match (left, right) {
            (FSNode::File(m0, h0), FSNode::File(m1, h1)) => m0 == m1 && h0 == h1,
            (FSNode::SymLink(h0), FSNode::SymLink(h1)) => h0 == h1,
            (FSNode::Dir(m0, _, subsub0), FSNode::Dir(m1, _, subsub1)) => {
                m0 == m1 && compatilble_added_subtrees(subsub0, subsub1)
            }
            _ => false,
        };

        if !compatible {
            return false;
        }
    }
    true
}

pub fn check_for_conflicts(delta0: &DeltaFSTree, delta1: &DeltaFSTree) -> Option<ConflictTree> {
    use ConflictNode as CN;
    use DeltaFSNode as DFS;
    use FSNode::*;

    // // TODO eeeeeeeeeeeeeh I'm not really a big fan of having to do these checks
    // //	but the inner match would look much worse if we allow unshaken deltas
    // //	sooo...
    // if !delta0.is_shaken() {
    //     panic!("First argument was not shaken");
    // }
    // if !delta1.is_shaken() {
    //     panic!("Second argument was not shaken");
    // }

    let DeltaFSTree(tree0) = delta0;
    let DeltaFSTree(tree1) = delta1;
    let mut conflicts: HashMap<String, ConflictNode> = HashMap::new();
    for (name, (left, right)) in intersect(tree0, tree1) {
        let conflict: Option<ConflictNode> = match (left, right) {
            (left, right) if left == right => None,
            (DFS::Branch(optm0, subdelta0), DFS::Branch(optm1, subdelta1)) => {
                let subconflicts = check_for_conflicts(subdelta0, subdelta1);
                let mconflict = match (optm0, optm1) {
                    (Some(m0), Some(m1)) if m0 != m1 => Some((m0.clone(), m1.clone())),
                    _ => None,
                };
                let ior = IOr::from(mconflict, subconflicts);
                if let Some(branch) = ior {
                    Some(CN::Branch(branch))
                } else {
                    None
                }
            }
            (DFS::Leaf(pre0, _), DFS::Leaf(pre1, _)) if pre0 != pre1 => {
                Some(CN::Leaf(left.clone(), right.clone()))
            }
            (DFS::Leaf(_, Some(Dir(_, _, subtree0))), DFS::Leaf(_, Some(Dir(_, _, subtree1)))) => {
                if !compatilble_added_subtrees(subtree0, subtree1) {
                    Some(CN::Leaf(left.clone(), right.clone()))
                } else {
                    None
                }
            }

            // By this point in the match, there are two possible cases:
            //	A) left and right mismatch (either Branch and Leaf or Leaf and Branch)
            //	B) left and right are both leaves
            // We can't have left and right both branches because that gets matched by the
            //	first match pattern
            // Any instance of (A) is a conflict. WLOG, an instance of (A) is
            //	(Leaf, Branch). If the post state of the leaf isn't a directory, then the
            //	post state of left and right is different and hence a conflict
            //	If the post state IS a directory, then either the pre state was a
            //	directory too, which would be an unshaken node which is counted as a
            //	conflict, or it's something else, in which case the pre state of left
            //	and right is different (a branch assumes the pre state is a directory)
            // In an instance of (B), since we didn't match any previous pattern, we
            //	know that:
            //	- pre0 == pre1
            //	- left != right
            //	which implies post0 != post1. Furthermore, we know that at least one of
            //	the post states is not a directory, which means it's either None or
            //	a filelike. In both cases, to not be a conflict it should be the same as
            //	the other post state, which it isnt't
            // Hence, any possibility this far in the match is a conflict
            _ => Some(ConflictNode::Leaf(left.clone(), right.clone())),
        };

        if let Some(node) = conflict {
            conflicts.insert(name, node);
        }
    }

    if conflicts.len() > 0 {
        Some(ConflictTree(conflicts))
    } else {
        None
    }
}
