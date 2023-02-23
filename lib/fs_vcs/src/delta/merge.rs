use super::{Delta, DeltaNode, FSNode};

use abst_fs::AbstPath;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
#[error("File System Tree Delta Error: unable to merge deltas.\nConflict at path: {0}\nError: {1}")]
pub struct UnmergeableDelta(AbstPath, String);
fn unmergerr<S>(path: AbstPath, err: S) -> UnmergeableDelta
where
    S: std::string::ToString,
{
    UnmergeableDelta(path, err.to_string())
}
fn push_unmerg<S>(parent: S) -> impl Fn(UnmergeableDelta) -> UnmergeableDelta
where
    S: std::string::ToString,
{
    move |UnmergeableDelta(path, err)| UnmergeableDelta(path.add_first(parent.to_string()), err)
}

impl Delta {
    // TODO add public setup function and private recursive function with
    // additional parameter to make the path of the error more precise (full
    // path)
    pub fn merge_prec(&mut self, prec: &Delta) -> Result<(), UnmergeableDelta> {
        use std::collections::hash_map::Entry::*;
        use DeltaNode::*;

        for (name, child_prec) in prec {
            match self.entry(name.clone()) {
                Vacant(entry) => {
                    entry.insert(child_prec.clone());
                }
                Occupied(mut entry) => match (child_prec, entry.get_mut()) {
                    (
                        Branch((premtime0, postmtime0), subdelta0),
                        Branch((premtime1, _), subdelta1),
                    ) => {
                        if postmtime0 != premtime1 {
                            return Err(unmergerr(AbstPath::single(name), "new mtime of precedent delta does not match with old mtime of successive delta"));
                        }
                        *premtime1 = premtime0.clone();
                        subdelta1.merge_prec(subdelta0).map_err(push_unmerg(name))?;
                    }
                    (Leaf(pre0, post0), Leaf(pre1, _)) => {
                        if post0 == pre1 {
                            *pre1 = pre0.clone();
                        } else {
                            return Err(unmergerr(AbstPath::single(name), "post state of precedent delta does not match with pre state of successive delta"));
                        }
                    }
                    (Leaf(pre0, post0), Branch((premtime1, postmtime1), subdelta1)) => {
                        match post0 {
                            Some(FSNode::Dir(mtime, _, subtree)) => {
                                let mut subtree = subtree.clone();
                                subtree
								.apply_delta(subdelta1)
								.map_err(|_| unmergerr(AbstPath::single(name), "failed to apply subdelta of successive delta branch to precedent delta's directory leaf"))?;
                                if mtime != premtime1 {
                                    return Err(unmergerr(AbstPath::single(name), "new mtime of precedent delta does not match with mtime of successive delta"));
                                }
                                let mtime = postmtime1.clone();
                                let hash = subtree.hash_tree();
                                entry.insert(Leaf(
                                    pre0.clone(),
                                    Some(FSNode::Dir(mtime, hash, subtree)),
                                ));
                            }
                            _ => {
                                return Err(unmergerr(AbstPath::single(name), "cannot merge branch delta (successive) with non dir leaf (precedent)"));
                            }
                        }
                    }
                    (Branch((premtime0, postmtime0), subdelta0), Leaf(pre1, _)) => match pre1 {
                        Some(FSNode::Dir(mtime, hash, subtree)) => {
                            subtree.undo_delta(subdelta0).map_err(|_| unmergerr(AbstPath::single(name), "failed to undo subdelta of precedent delta branch to successive delta's directory leaf"))?;
                            *hash = subtree.hash_tree();
                            if postmtime0 != mtime {
                                return Err(unmergerr(AbstPath::single(name), "new metadata of precedent delta does not match with metadata of successive delta"));
                            }
                            *mtime = premtime0.clone();
                        }
                        _ => {
                            return Err(unmergerr(AbstPath::single(name), "cannot merge non dir leaf (successive) with branch delta (successive)"));
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
                match self.get(name) {
                    None => None,
                    Some(DeltaNode::Branch(_, subdelta)) => {
                        subdelta.get_subdelta_tree_copy(&path.strip_first())
                    }
                    Some(DeltaNode::Leaf(None, Some(FSNode::Dir(_, _, subtree)))) => {
                        let mut subdelta = Delta::new();
                        for (node, child) in subtree {
                            subdelta
                                .insert(node.clone(), DeltaNode::Leaf(None, Some(child.clone())));
                        }
                        subdelta.get_subdelta_tree_copy(&path.strip_first())
                    }
                    Some(DeltaNode::Leaf(Some(FSNode::Dir(_, _, subtree)), None)) => {
                        let mut subdelta = Delta::new();
                        for (node, child) in subtree {
                            subdelta
                                .insert(node.clone(), DeltaNode::Leaf(Some(child.clone()), None));
                        }
                        subdelta.get_subdelta_tree_copy(&path.strip_first())
                    }

                    // I think this assumes that the delta is shaken
                    Some(DeltaNode::Leaf(_, _)) => None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use abst_fs::AbstPath;

    use super::{super::FSTree, push_unmerg, unmergerr, Delta, FSNode, UnmergeableDelta};

    #[test]
    fn test_error() {
        let err = unmergerr(AbstPath::from("name"), "some error");
        assert_eq!(
            UnmergeableDelta(AbstPath::from("name"), String::from("some error")),
            err
        );
        assert_eq!(
            UnmergeableDelta(AbstPath::from("parent/name"), String::from("some error")),
            push_unmerg("parent")(err)
        );
    }

    #[test]
    fn test_merge() {
        // There are four states in which an object can exist inside an FSTree:
        // None, Some(File), Some(Symlink) and Some(Dir). A delta is basically
        // a pair of states (initial state and final state) of that object. A
        // pair of delta is basically a triplet of states (initial state, mid
        // state and final state) of the object. We will now create a pair of
        // delta that contains all the possible triplets.
        // An object called "file-file-none" will represent an object that
        // starts as a file, gets edited into a different file in the first
        // delta and gets removed in the second delta.

        let pre = [
            ("none", None),
            (
                "file",
                Some(FSNode::file((1664606882, 650210987), "content 0")),
            ),
            (
                "symlink",
                Some(FSNode::symlink((1664625081, 899946721), "path/to/0")),
            ),
            (
                "dir",
                Some(FSNode::dir((1664682506, 502056697), |t| {
                    t.add_file("some-file", (1664646746, 532084290), "content 1");
                    t.add_symlink("some-symlink", (1664661848, 602576901), "path/to/1");
                    t.add_empty_dir("some-dir", (1664674840, 688825981));
                })),
            ),
        ];
        let mid = [
            ("none", None),
            (
                "file",
                Some(FSNode::file((1664688708, 831226577), "content 2")),
            ),
            (
                "symlink",
                Some(FSNode::symlink((1664729718, 723043252), "path/to/2")),
            ),
            (
                "dir",
                Some(FSNode::dir((1664858680, 525212767), |t| {
                    t.add_file("some-file", (1664778889, 378546643), "content 3");
                    t.add_symlink("some-symlink", (1664803756, 119510765), "path/to/3");
                    t.add_empty_dir("some-dir", (1664848992, 505965488));
                })),
            ),
        ];
        let post = [
            ("none", None),
            (
                "file",
                Some(FSNode::file((1664891266, 931780439), "content 4")),
            ),
            (
                "symlink",
                Some(FSNode::symlink((1664898305, 223815908), "path/to/4")),
            ),
            (
                "dir",
                Some(FSNode::dir((1664997583, 776691130), |t| {
                    t.add_file("some-file", (1664935309, 24083582), "content 5");
                    t.add_symlink("some-symlink", (1664954522, 993865018), "path/to/5");
                    t.add_empty_dir("some-dir", (1664982104, 716018057));
                })),
            ),
        ];

        let mut pre_fstree = FSTree::new();
        let mut mid_fstree = FSTree::new();
        let mut post_fstree = FSTree::new();
        for (pre_type, pre_node) in &pre {
            for (mid_type, mid_node) in &mid {
                for (post_type, post_node) in &post {
                    let name = format!("{pre_type}-{mid_type}-{post_type}");

                    if let Some(node) = pre_node {
                        pre_fstree.insert(name.clone(), node.clone());
                    }
                    if let Some(node) = mid_node {
                        mid_fstree.insert(name.clone(), node.clone());
                    }
                    if let Some(node) = post_node {
                        post_fstree.insert(name.clone(), node.clone());
                    }
                }

                // This part is to make sure that objects that deltas about
                // objects that get changed and then reverted to original get
                // deleted in the merge as effectively the total delta does
                // not change said object
                let name = format!("{pre_type}-{mid_type}-old{pre_type}");

                if let Some(node) = pre_node {
                    pre_fstree.insert(name.clone(), node.clone());
                    post_fstree.insert(name.clone(), node.clone());
                }
                if let Some(node) = mid_node {
                    mid_fstree.insert(name.clone(), node.clone());
                }
            }
        }

        let prec_delta = pre_fstree.get_delta_to(&mid_fstree);
        let mut succ_delta = mid_fstree.get_delta_to(&post_fstree);
        let total_delta = pre_fstree.get_delta_to(&post_fstree);

        succ_delta.merge_prec(&prec_delta).unwrap();
        assert_eq!(succ_delta, total_delta);
    }

    #[test]
    fn test_merge_error() {
        // Wrong leaf post0, pre1 (existing/nonexisting, mismatching or wrong)
        let pre = [
            None,
            Some(FSNode::file((1664590305, 648768843), "content 0")),
            Some(FSNode::symlink((1664607464, 223391226), "path/to/0")),
            Some(FSNode::dir((1664706448, 871350601), |t| {
                t.add_file("some-file", (1664654009, 36535473), "content 1");
                t.add_symlink("some-symlink", (1664670155, 581396374), "path/to/1");
                t.add_empty_dir("some-dir", (1664678903, 825411342));
            })),
        ];
        let mid_1 = [
            None,
            Some(FSNode::file((1664732150, 816499870), "content 2")),
            Some(FSNode::symlink((1664744953, 143739934), "path/to/2")),
            Some(FSNode::dir((1664818488, 958261489), |t| {
                t.add_file("some-file", (1664757442, 427289926), "content 3");
                t.add_symlink("some-symlink", (1664766153, 339479581), "path/to/3");
                t.add_empty_dir("some-dir", (1664800533, 433225235));
            })),
        ];
        let mid_2 = [
            None,
            Some(FSNode::file((1664856996, 55239376), "content 4")),
            Some(FSNode::symlink((1664874354, 319493207), "path/to/4")),
            Some(FSNode::dir((1664940824, 121523023), |t| {
                t.add_file("some-file", (1664880822, 366993072), "content 5");
                t.add_symlink("some-symlink", (1664916275, 806650459), "path/to/5");
                t.add_empty_dir("some-dir", (1664924516, 487361010));
            })),
        ];
        let post = [
            None,
            Some(FSNode::file((1664989787, 201175353), "content 6")),
            Some(FSNode::symlink((1665003649, 81596856), "path/to/6")),
            Some(FSNode::dir((1665132254, 741051682), |t| {
                t.add_file("some-file", (1665051410, 655625476), "content 3");
                t.add_symlink("some-symlink", (1665075120, 602812525), "path/to/3");
                t.add_empty_dir("some-dir", (1665104664, 708032682));
            })),
        ];

        for pre_node in &pre {
            for mid_1_node in &mid_1 {
                for mid_2_node in &mid_2 {
                    for post_node in &post {
                        // Let's break down the following guards:
                        // - mid_1_node.is_some() || mid_2_node.is_some()
                        //      at least one of the two midstates has to be
                        //      Some because if they're both None then there is
                        //      no mismatching or wrong object going on, that
                        //      would be just deleting an object and creating
                        //      another in its place, which is a valid
                        //      behaviour
                        // - pre_node.is_some() || mid_1_node.is_some()
                        //      at least one of the two states of the
                        //      prec_delta has to be some because if they're
                        //      both None then the prec_delta is empty and is
                        //      therefore compatible with any succ_delta
                        // - mid_2_node.is_some() || post_node.is_some()
                        //      same reasoning as the previous guard
                        if (mid_1_node.is_some() || mid_2_node.is_some())
                            && (pre_node.is_some() || mid_1_node.is_some())
                            && (mid_2_node.is_some() || post_node.is_some())
                        {
                            let pre_fstree = match pre_node {
                                None => FSTree::new(),
                                Some(node) => FSTree::gen_from(|t| {
                                    t.insert("object", node.clone());
                                }),
                            };
                            let mid_1_fstree = match mid_1_node {
                                None => FSTree::new(),
                                Some(node) => FSTree::gen_from(|t| {
                                    t.insert("object", node.clone());
                                }),
                            };
                            let mid_2_fstree = match mid_2_node {
                                None => FSTree::new(),
                                Some(node) => FSTree::gen_from(|t| {
                                    t.insert("object", node.clone());
                                }),
                            };
                            let post_fstree = match post_node {
                                None => FSTree::new(),
                                Some(node) => FSTree::gen_from(|t| {
                                    t.insert("object", node.clone());
                                }),
                            };

                            let prec_delta = pre_fstree.get_delta_to(&mid_1_fstree);
                            let mut succ_delta = mid_2_fstree.get_delta_to(&post_fstree);
                            assert!(succ_delta.merge_prec(&prec_delta).is_err());
                        }
                    }
                }
            }
        }

        // Nested error
        let prec_delta = Delta::gen_from(|d| {
            d.add_branch(
                "dir",
                ((1665174601, 548198391), (1665209483, 852333001)),
                |d| {
                    d.add_leaf(
                        "file",
                        None,
                        Some(FSNode::file((1665148898, 367694017), "content 0")),
                    );
                },
            );
        });
        let mut succ_delta = Delta::gen_from(|d| {
            d.add_branch(
                "dir",
                ((1665209483, 852333001), (1665302960, 387756074)),
                |d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1665247521, 128612996), "content 1")),
                        Some(FSNode::file((1665269806, 237275940), "content 2")),
                    );
                },
            );
        });
        assert!(succ_delta.merge_prec(&prec_delta).is_err());
    }

    #[test]
    fn test_subdelta_tree() {
        let delta = Delta::gen_from(|d| {
            d.add_branch(
                "path",
                ((1665023819, 356526576), (1665369758, 725521665)),
                |d| {
                    d.add_branch(
                        "to",
                        ((1664985625, 116366659), (1665323455, 909172992)),
                        |d| {
                            d.add_branch(
                                "somewhere",
                                ((1664736554, 869045099), (1665173730, 551392714)),
                                |d| {
                                    d.add_leaf(
                                        "old-file",
                                        Some(FSNode::file((1664613202, 830966602), "content 0")),
                                        None,
                                    );
                                    d.add_leaf(
                                        "old-symlink",
                                        Some(FSNode::symlink((1664628389, 889122338), "path/to/0")),
                                        None,
                                    );
                                    d.add_leaf(
                                        "old-dir",
                                        Some(FSNode::dir((1664702802, 796843211), |t| {
                                            t.add_file(
                                                "some-file",
                                                (1664664052, 629441456),
                                                "content 1",
                                            );
                                            t.add_symlink(
                                                "some-symlink",
                                                (1664679922, 100679229),
                                                "path/to/1",
                                            );
                                            t.add_empty_dir("some-dir", (1664686245, 984602116));
                                        })),
                                        None,
                                    );
                                    d.add_leaf(
                                        "new-file",
                                        None,
                                        Some(FSNode::file((1665031266, 679056252), "content 2")),
                                    );
                                    d.add_leaf(
                                        "new-symlink",
                                        None,
                                        Some(FSNode::symlink((1665077293, 436086456), "path/to/2")),
                                    );
                                    d.add_leaf(
                                        "new-dir",
                                        None,
                                        Some(FSNode::empty_dir((1665091794, 188717306))),
                                    );
                                },
                            );
                            d.add_branch(
                                "new",
                                ((1664785722, 448231979), (1665269289, 252217136)),
                                |d| {
                                    d.add_leaf(
                                        "dir",
                                        None,
                                        Some(FSNode::dir((1665256237, 734756460), |t| {
                                            t.add_file(
                                                "some-file",
                                                (1665194696, 846252069),
                                                "content 3",
                                            );
                                            t.add_symlink(
                                                "some-symlink",
                                                (1665212881, 695443303),
                                                "path/to/3",
                                            );
                                            t.add_empty_dir("some-dir", (1665242775, 322518411));
                                        })),
                                    );
                                },
                            );
                            d.add_branch(
                                "old",
                                ((1664934804, 287245727), (1665293022, 200825086)),
                                |d| {
                                    d.add_leaf(
                                        "dir",
                                        Some(FSNode::dir((1664889419, 740090713), |t| {
                                            t.add_file(
                                                "some-file",
                                                (1664819756, 563532835),
                                                "content 4",
                                            );
                                            t.add_symlink(
                                                "some-symlink",
                                                (1664828993, 352188126),
                                                "path/to/4",
                                            );
                                            t.add_empty_dir("some-dir", (1664842021, 681499423));
                                        })),
                                        None,
                                    );
                                },
                            );
                            d.add_leaf(
                                "file",
                                Some(FSNode::file((1664948235, 972227892), "content 5")),
                                Some(FSNode::file((1665317543, 691944051), "content 6")),
                            );
                        },
                    );
                },
            );
        });

        assert_eq!(
            delta.get_subdelta_tree_copy(&AbstPath::from("path/to/somewhere")),
            Some(Delta::gen_from(|d| {
                d.add_leaf(
                    "old-file",
                    Some(FSNode::file((1664613202, 830966602), "content 0")),
                    None,
                );
                d.add_leaf(
                    "old-symlink",
                    Some(FSNode::symlink((1664628389, 889122338), "path/to/0")),
                    None,
                );
                d.add_leaf(
                    "old-dir",
                    Some(FSNode::dir((1664702802, 796843211), |t| {
                        t.add_file("some-file", (1664664052, 629441456), "content 1");
                        t.add_symlink("some-symlink", (1664679922, 100679229), "path/to/1");
                        t.add_empty_dir("some-dir", (1664686245, 984602116));
                    })),
                    None,
                );
                d.add_leaf(
                    "new-file",
                    None,
                    Some(FSNode::file((1665031266, 679056252), "content 2")),
                );
                d.add_leaf(
                    "new-symlink",
                    None,
                    Some(FSNode::symlink((1665077293, 436086456), "path/to/2")),
                );
                d.add_leaf(
                    "new-dir",
                    None,
                    Some(FSNode::empty_dir((1665091794, 188717306))),
                );
            }))
        );

        assert_eq!(
            delta.get_subdelta_tree_copy(&AbstPath::from("path/to/new/dir")),
            Some(Delta::gen_from(|d| {
                d.add_leaf(
                    "some-file",
                    None,
                    Some(FSNode::file((1665194696, 846252069), "content 3")),
                );
                d.add_leaf(
                    "some-symlink",
                    None,
                    Some(FSNode::symlink((1665212881, 695443303), "path/to/3")),
                );
                d.add_leaf(
                    "some-dir",
                    None,
                    Some(FSNode::empty_dir((1665242775, 322518411))),
                );
            }))
        );

        assert_eq!(
            delta.get_subdelta_tree_copy(&AbstPath::from("path/to/old/dir")),
            Some(Delta::gen_from(|d| {
                d.add_leaf(
                    "some-file",
                    Some(FSNode::file((1664819756, 563532835), "content 4")),
                    None,
                );
                d.add_leaf(
                    "some-symlink",
                    Some(FSNode::symlink((1664828993, 352188126), "path/to/4")),
                    None,
                );
                d.add_leaf(
                    "some-dir",
                    Some(FSNode::empty_dir((1664842021, 681499423))),
                    None,
                );
            }))
        );

        assert_eq!(
            delta.get_subdelta_tree_copy(&AbstPath::from("path/to/file")),
            None
        );

        assert_eq!(
            delta.get_subdelta_tree_copy(&AbstPath::from("non/existing/path")),
            None
        );
    }
}
