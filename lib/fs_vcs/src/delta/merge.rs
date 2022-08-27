use super::{hash_tree, Delta, DeltaNode, FSNode, FSTree};

use abst_fs::AbstPath;
use std::collections::HashMap;
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
                                    return Err(unmergerr(AbstPath::single(name), "new mtime of precedent delta does not match with old mtime of successive delta"));
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
                            return Err(unmergerr(AbstPath::single(name), "post state of precedent delta does not match with pre state of successive delta"));
                        }
                    }
                    (Leaf(pre0, post0), Branch(optm1, subdelta1)) => match post0 {
                        Some(FSNode::Dir(mtime, _, subtree)) => {
                            let mut subtree = subtree.clone();
                            subtree
								.apply_delta(subdelta1)
								.map_err(|_| unmergerr(AbstPath::single(name), "failed to apply subdelta of successive delta branch to precedent delta's directory leaf"))?;
                            let mtime = match optm1 {
                                Some((premtime1, postmtime1)) => {
                                    if mtime != premtime1 {
                                        return Err(unmergerr(AbstPath::single(name), "new mtime of precedent delta does not match with mtime of successive delta"));
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
                            return Err(unmergerr(AbstPath::single(name), "cannot merge branch delta (successive) with non dir leaf (precedent)"));
                        }
                    },
                    (Branch(optm0, subdelta0), Leaf(pre1, _)) => match pre1 {
                        Some(FSNode::Dir(mtime, hash, subtree)) => {
                            subtree.undo_delta(subdelta0).map_err(|_| unmergerr(AbstPath::single(name), "failed to undo subdelta of precedent delta branch to successive delta's directory leaf"))?;
                            *hash = hash_tree(subtree);
                            if let Some((premtime0, postmtime0)) = optm0 {
                                if postmtime0 != mtime {
                                    return Err(unmergerr(AbstPath::single(name), "new metadata of precedent delta does not match with metadata of successive delta"));
                                }
                                *mtime = premtime0.clone();
                            }
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

#[cfg(test)]
mod tests {
    use abst_fs::AbstPath;

    use super::{push_unmerg, unmergerr, Delta, FSNode, UnmergeableDelta};

    #[test]
    fn test() {
        error();

        merge();

        subdelta_tree();
    }

    fn error() {
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

    fn merge() {
        // Correct
        {
            let prec_delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "only-in-prec",
                    None,
                    Some(FSNode::file((511296105, 388548694), "also content")),
                );
                d.add_branch(
                    "branch1",
                    Some(((511296105, 388548694), (514319706, 941690660))),
                    |d| {
                        d.add_leaf(
                            "file2",
                            Some(FSNode::file((671604137, 286842258), "some words")),
                            Some(FSNode::file((760709528, 559907222), "different words")),
                        );
                    },
                );
                d.add_branch(
                    "branch2",
                    Some(((1265546486, 389473436), (1396487263, 534084134))),
                    |d| {
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1211083411, 596465489), "old content")),
                            Some(FSNode::file((1363282625, 548908666), "new content")),
                        );
                    },
                );
                d.add_branch("branch3", None, |d| {
                    d.add_leaf(
                        "file",
                        None,
                        Some(FSNode::file((1478623745, 355441118), "some content")),
                    );
                });
                d.add_branch("branch4", None, |d| {
                    d.add_leaf(
                        "file",
                        None,
                        Some(FSNode::file((897624970, 16191305), "qwertyuiop")),
                    );
                });
                d.add_branch("disappearing-branch", None, |d| {
                    d.add_leaf(
                        "disappearing-file",
                        None,
                        Some(FSNode::file((1247543129, 922667729), "zxcvbnm")),
                    );
                });
                d.add_leaf(
                    "leaf-then-branch",
                    None,
                    Some(FSNode::dir((1013348856, 129427648), |t| {
                        t.add_file("file", (1482393259, 727391402), "something");
                    })),
                );
                d.add_leaf(
                    "leaf-then-branch-and-mtime",
                    None,
                    Some(FSNode::dir((1325817515, 133346763), |t| {
                        t.add_symlink("symlink", (1458719618, 94382097), "path/to/somewhere");
                    })),
                );
                d.add_branch(
                    "branch-then-leaf",
                    Some(((1446120388, 364679759), (1481801479, 584963489))),
                    |d| {
                        d.add_leaf(
                            "dir",
                            Some(FSNode::empty_dir((1533247505, 465403976))),
                            None,
                        );
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1535512117, 790483817), "text here")),
                            Some(FSNode::file((1535751095, 541556742), "text there")),
                        );
                    },
                );
            });
            let mut succ_delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "only-in-succ",
                    None,
                    Some(FSNode::file((936632440, 451303481), "content")),
                );
                d.add_branch(
                    "branch1",
                    Some(((514319706, 941690660), (926865806, 601491695))),
                    |d| {
                        d.add_leaf(
                            "file1",
                            Some(FSNode::file((814015932, 633808148), "other content")),
                            None,
                        );
                    },
                );
                d.add_branch("branch2", None, |d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1363282625, 548908666), "new content")),
                        None,
                    );
                });
                d.add_branch(
                    "branch3",
                    Some(((1274629696, 436917207), (1380380585, 150731897))),
                    |d| {
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1478623745, 355441118), "some content")),
                            Some(FSNode::file((1482393259, 727391402), "some content")),
                        );
                    },
                );
                d.add_branch("branch4", None, |d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((897624970, 16191305), "qwertyuiop")),
                        Some(FSNode::file((897624970, 16191305), "asdfghjkl")),
                    );
                });
                d.add_branch("disappearing-branch", None, |d| {
                    d.add_leaf(
                        "disappearing-file",
                        Some(FSNode::file((1247543129, 922667729), "zxcvbnm")),
                        None,
                    );
                });
                d.add_branch("leaf-then-branch", None, |d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1482393259, 727391402), "something")),
                        Some(FSNode::file((1370673776, 14576189), "else")),
                    )
                });
                d.add_branch(
                    "leaf-then-branch-and-mtime",
                    Some(((1325817515, 133346763), (1547840721, 598700627))),
                    |d| {
                        d.add_leaf(
                            "symlink",
                            Some(FSNode::symlink((1458719618, 94382097), "path/to/somewhere")),
                            Some(FSNode::symlink(
                                (1545042749, 17582128),
                                "some/different/path",
                            )),
                        )
                    },
                );
                d.add_leaf(
                    "branch-then-leaf",
                    Some(FSNode::dir((1481801479, 584963489), |t| {
                        t.add_file("file", (1535751095, 541556742), "text there");
                    })),
                    None,
                );
            });

            let merged_delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "only-in-succ",
                    None,
                    Some(FSNode::file((936632440, 451303481), "content")),
                );
                d.add_leaf(
                    "only-in-prec",
                    None,
                    Some(FSNode::file((511296105, 388548694), "also content")),
                );
                d.add_branch(
                    "branch1",
                    Some(((511296105, 388548694), (926865806, 601491695))),
                    |d| {
                        d.add_leaf(
                            "file1",
                            Some(FSNode::file((814015932, 633808148), "other content")),
                            None,
                        );
                        d.add_leaf(
                            "file2",
                            Some(FSNode::file((671604137, 286842258), "some words")),
                            Some(FSNode::file((760709528, 559907222), "different words")),
                        );
                    },
                );
                d.add_branch(
                    "branch2",
                    Some(((1265546486, 389473436), (1396487263, 534084134))),
                    |d| {
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1211083411, 596465489), "old content")),
                            None,
                        );
                    },
                );
                d.add_branch(
                    "branch3",
                    Some(((1274629696, 436917207), (1380380585, 150731897))),
                    |d| {
                        d.add_leaf(
                            "file",
                            None,
                            Some(FSNode::file((1482393259, 727391402), "some content")),
                        );
                    },
                );
                d.add_branch("branch4", None, |d| {
                    d.add_leaf(
                        "file",
                        None,
                        Some(FSNode::file((897624970, 16191305), "asdfghjkl")),
                    );
                });
                d.add_leaf(
                    "leaf-then-branch",
                    None,
                    Some(FSNode::dir((1013348856, 129427648), |t| {
                        t.add_file("file", (1370673776, 14576189), "else");
                    })),
                );
                d.add_leaf(
                    "leaf-then-branch-and-mtime",
                    None,
                    Some(FSNode::dir((1547840721, 598700627), |t| {
                        t.add_symlink("symlink", (1545042749, 17582128), "some/different/path");
                    })),
                );
                d.add_leaf(
                    "branch-then-leaf",
                    Some(FSNode::dir((1446120388, 364679759), |t| {
                        t.add_empty_dir("dir", (1533247505, 465403976));
                        t.add_file("file", (1535512117, 790483817), "text here");
                    })),
                    None,
                );
            });
            succ_delta.merge_prec(&prec_delta).unwrap();
            assert_eq!(succ_delta, merged_delta);
        }

        // Wrong branch postmtime0 premtime1
        {
            let prec_delta = Delta::gen_from(|d| {
                d.add_empty_branch(
                    "dir",
                    Some(((1313725245, 996812438), (1437259585, 759869682))),
                );
            });
            let mut succ_delta = Delta::gen_from(|d| {
                d.add_empty_branch(
                    "dir",
                    Some(((1451353431, 78039314), (1568981165, 501778746))),
                );
            });

            assert!(succ_delta.merge_prec(&prec_delta).is_err());
        }

        // Wrong leaf post0, pre1
        {
            let prec_delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "file",
                    None,
                    Some(FSNode::file((1350665172, 148856944), "eooo")),
                );
            });
            let mut succ_delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "file",
                    Some(FSNode::file((1266391166, 948545626), "iaaa")),
                    None,
                );
            });

            assert!(succ_delta.merge_prec(&prec_delta).is_err());
        }

        // Wrong leaf then branch mtime postmtime
        {
            let prec_delta = Delta::gen_from(|d| {
                d.add_leaf("dir", None, Some(FSNode::empty_dir((999704909, 180137536))));
            });
            let mut succ_delta = Delta::gen_from(|d| {
                d.add_empty_branch(
                    "dir",
                    Some(((927155718, 657468981), (972110831, 154383606))),
                );
            });

            assert!(succ_delta.merge_prec(&prec_delta).is_err());
        }

        // Wrong branch then leaf postmtime mtime
        {
            let prec_delta = Delta::gen_from(|d| {
                d.add_empty_branch(
                    "dir",
                    Some(((927155718, 657468981), (972110831, 154383606))),
                );
            });
            let mut succ_delta = Delta::gen_from(|d| {
                d.add_leaf("dir", Some(FSNode::empty_dir((999704909, 180137536))), None);
            });

            assert!(succ_delta.merge_prec(&prec_delta).is_err());
        }

        // Wrong branch then non dir
        {
            let prec_delta = Delta::gen_from(|d| {
                d.add_empty_branch(
                    "dir",
                    Some(((927155718, 657468981), (972110831, 154383606))),
                );
            });
            let mut succ_delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "dir",
                    Some(FSNode::file((1592510606, 326647757), "hey")),
                    None,
                );
            });

            assert!(succ_delta.merge_prec(&prec_delta).is_err());
        }

        // Wrong leaf then branch mtime postmtime
        {
            let prec_delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "dir",
                    None,
                    Some(FSNode::file((999704909, 180137536), "uuuuu")),
                );
            });
            let mut succ_delta = Delta::gen_from(|d| {
                d.add_empty_branch(
                    "dir",
                    Some(((927155718, 657468981), (972110831, 154383606))),
                );
            });

            assert!(succ_delta.merge_prec(&prec_delta).is_err());
        }
    }

    fn subdelta_tree() {
        // Correct
        {
            let delta = Delta::gen_from(|d| {
                d.add_branch("path", None, |d| {
                    d.add_branch("to", None, |d| {
                        d.add_branch("somewhere", None, |d| {
                            d.add_empty_branch(
                                "some-dir",
                                Some(((950699263, 653349505), (999704909, 180137536))),
                            );
                            d.add_leaf(
                                "some-file",
                                None,
                                Some(FSNode::file((1482493363, 159242111), "content")),
                            );
                        });
                        d.add_branch("new", None, |d| {
                            d.add_leaf(
                                "dir",
                                None,
                                Some(FSNode::dir((1343075884, 988762165), |t| {
                                    t.add_file("file", (1302727190, 651605513), "test content");
                                    t.add_symlink(
                                        "symlink",
                                        (1280173113, 825083229),
                                        "some/random/endpoint",
                                    );
                                    t.add_dir("subdir", (1280173113, 825083229), |t| {
                                        t.add_file("file2", (1305477967, 139210700), "...");
                                        t.add_empty_dir("subsubdir", (1289161313, 113559192));
                                    });
                                })),
                            )
                        });
                        d.add_branch("old", None, |d| {
                            d.add_leaf(
                                "dir",
                                Some(FSNode::dir((1312686096, 98837056), |t| {
                                    t.add_file(
                                        "file",
                                        (1418494687, 489957216),
                                        "actual content :)",
                                    );
                                    t.add_symlink(
                                        "symlink",
                                        (1464130134, 865466751),
                                        "other/random/endpoint",
                                    );
                                    t.add_dir("subdir", (1280267826, 170987648), |t| {
                                        t.add_file("file2", (1451801950, 961804716), "!!!");
                                        t.add_empty_dir("subsubdir", (1401825462, 746027656));
                                    });
                                })),
                                None,
                            )
                        });
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1215595033, 169443440), "boring")),
                            Some(FSNode::file((1226600804, 801330900), "interesting")),
                        );
                    });
                });
                d.add_branch("non", None, |d| {
                    d.add_empty_branch(
                        "existing",
                        Some(((1234963364, 638624210), (1283803802, 968704076))),
                    );
                });
            });

            assert_eq!(
                delta.get_subdelta_tree_copy(&AbstPath::from("path/to/somewhere")),
                Some(Delta::gen_from(|d| {
                    d.add_empty_branch(
                        "some-dir",
                        Some(((950699263, 653349505), (999704909, 180137536))),
                    );
                    d.add_leaf(
                        "some-file",
                        None,
                        Some(FSNode::file((1482493363, 159242111), "content")),
                    );
                }))
            );

            assert_eq!(
                delta.get_subdelta_tree_copy(&AbstPath::from("path/to/new/dir")),
                Some(Delta::gen_from(|d| {
                    d.add_leaf(
                        "file",
                        None,
                        Some(FSNode::file((1302727190, 651605513), "test content")),
                    );
                    d.add_leaf(
                        "symlink",
                        None,
                        Some(FSNode::symlink(
                            (1280173113, 825083229),
                            "some/random/endpoint",
                        )),
                    );
                    d.add_leaf(
                        "subdir",
                        None,
                        Some(FSNode::dir((1280173113, 825083229), |t| {
                            t.add_file("file2", (1305477967, 139210700), "...");
                            t.add_empty_dir("subsubdir", (1289161313, 113559192));
                        })),
                    );
                }))
            );

            assert_eq!(
                delta.get_subdelta_tree_copy(&AbstPath::from("path/to/old/dir")),
                Some(Delta::gen_from(|d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1418494687, 489957216), "actual content :)")),
                        None,
                    );
                    d.add_leaf(
                        "symlink",
                        Some(FSNode::symlink(
                            (1464130134, 865466751),
                            "other/random/endpoint",
                        )),
                        None,
                    );
                    d.add_leaf(
                        "subdir",
                        Some(FSNode::dir((1280267826, 170987648), |t| {
                            t.add_file("file2", (1451801950, 961804716), "!!!");
                            t.add_empty_dir("subsubdir", (1401825462, 746027656));
                        })),
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
}
