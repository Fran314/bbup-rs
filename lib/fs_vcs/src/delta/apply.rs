use super::{hash_tree, Delta, DeltaNode, FSNode, FSTree};
use abst_fs::AbstPath;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
#[error(
    "File System Tree Delta Error: unable to apply delta to tree.\nConflict at path: {0}\nError: {1}"
)]
pub struct InapplicableDelta(AbstPath, String);
fn inapperr<S: std::string::ToString>(path: &AbstPath, err: S) -> InapplicableDelta {
    InapplicableDelta(path.clone(), err.to_string())
}
fn push_inapp<S: std::string::ToString>(
    parent: S,
) -> impl Fn(InapplicableDelta) -> InapplicableDelta {
    move |InapplicableDelta(path, err)| InapplicableDelta(path.add_first(parent.to_string()), err)
}

impl FSTree {
    pub fn apply_delta_at_endpoint(
        &mut self,
        delta: &Delta,
        endpoint: AbstPath,
    ) -> Result<(), InapplicableDelta> {
        // TODO if this was iterative instead of recursive, the error could be
        // more esplicit on the precise path of conflict, instad of just giving
        // the name of the node of conflict
        // OR MAYBE add a public setup function adn a private recursive function
        // like for the function below (pub fn apply_delta(self, &Delta))
        match endpoint.get(0) {
            None => self.apply_delta(delta),
            Some(name) => {
                let FSTree(fstree) = self;
                match fstree.get_mut(name) {
                    Some(FSNode::Dir(_, _, subtree)) => {
                        subtree.apply_delta_at_endpoint(delta, endpoint.strip_first())
                    }
                    Some(FSNode::File(_, _)) => Err(inapperr(
                        &AbstPath::single(name),
                        "endpoint claims this node is a directory, but it is a file",
                    )),
                    Some(FSNode::SymLink(_, _)) => Err(inapperr(
                        &AbstPath::single(name),
                        "endpoint claims this node is a directory, but it is a symlink",
                    )),
                    None => Err(inapperr(
                        &AbstPath::single(name),
                        "endpoint claims this node is a directory, but it doesn't exist",
                    )),
                }
            }
        }
    }

    // TODO add public setup function and private recursive function with
    // additional parameter to make the path of the error more precise (full
    // path)
    pub fn apply_delta(&mut self, Delta(deltatree): &Delta) -> Result<(), InapplicableDelta> {
        use std::collections::hash_map::Entry::{Occupied, Vacant};
        use DeltaNode::{Branch, Leaf};
        let FSTree(fstree) = self;
        for (name, child) in deltatree {
            match child {
                Leaf(None, None) => {
                    // This is an unshaken node, maybe I should say something about it
                    match fstree.entry(name.clone()) {
                        Vacant(_) => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                &AbstPath::single(name),
                                "delta claims this node is None, but it exists in tree",
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
                            &AbstPath::single(name),
                            "delta pre state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            &AbstPath::single(name),
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
                            &AbstPath::single(name),
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
                            &AbstPath::single(name),
                            "delta pre state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            &AbstPath::single(name),
                            "delta claims this node is Some, but it does not exist in tree",
                        ));
                    }
                },
                Branch((premtime, postmtime), subdelta) => {
                    match fstree.entry(name.clone()) {
                        Occupied(mut entry) => match entry.get_mut() {
                            FSNode::Dir(mtime, hash, subtree) => {
                                if mtime != premtime {
                                    return Err(inapperr(&AbstPath::single(name), "mtime of directory does not match old mtime of delta branch"));
                                }
                                *mtime = postmtime.clone();
                                subtree.apply_delta(subdelta).map_err(push_inapp(name))?;
                                *hash = hash_tree(subtree);
                            }
                            FSNode::File(_, _) => {
                                return Err(inapperr(
                                &AbstPath::single(name),
                                "delta claims this node is a directory, but it is a file in tree",
                            ));
                            }
                            FSNode::SymLink(_, _) => {
                                return Err(inapperr(
								&AbstPath::single(name),
								"delta claims this node is a directory, but it is a symlink in tree",
							));
                            }
                        },
                        Vacant(_) => {
                            return Err(inapperr(
                            &AbstPath::single(name),
                            "delta claims this node is a directory, but it does not exist in tree",
                        ));
                        }
                    }
                }
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
                    // This is an unshaken node, maybe I should say something about it
                    match fstree.entry(name.clone()) {
                        Vacant(_) => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                &AbstPath::single(name),
                                "delta claims this node is None, but it exists in tree",
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
                            &AbstPath::single(name),
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
                            &AbstPath::single(name),
                            "delta post state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            &AbstPath::single(name),
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
                            &AbstPath::single(name),
                            "delta post state for this node does not match with node in tree",
                        ));
                    }
                    Vacant(_) => {
                        return Err(inapperr(
                            &AbstPath::single(name),
                            "delta claims this node is Some, but it does not exist in tree",
                        ));
                    }
                },
                Branch((premtime, postmtime), subdelta) => {
                    match fstree.entry(name.clone()) {
                        Occupied(mut entry) => match entry.get_mut() {
                            FSNode::Dir(mtime, hash, subtree) => {
                                if mtime != postmtime {
                                    return Err(inapperr(&AbstPath::single(name), "mtime of directory does not match new mtime of delta branch"));
                                }
                                *mtime = premtime.clone();
                                subtree.undo_delta(subdelta).map_err(push_inapp(name))?;
                                *hash = hash_tree(subtree);
                            }
                            FSNode::File(_, _) => {
                                return Err(inapperr(
                                &AbstPath::single(name),
                                "delta claims this node is a directory, but it is a file in tree",
                            ));
                            }
                            FSNode::SymLink(_, _) => {
                                return Err(inapperr(
									&AbstPath::single(name),
									"delta claims this node is a directory, but it is a symlink in tree",
								));
                            }
                        },
                        Vacant(_) => {
                            return Err(inapperr(
                            &AbstPath::single(name),
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

#[cfg(test)]
mod tests {
    use super::{inapperr, push_inapp, Delta, FSNode, FSTree, InapplicableDelta};
    use abst_fs::AbstPath;

    #[test]
    fn test() {
        error();

        apply();
        undo();
        apply_at_endpoint();
    }

    fn error() {
        let err = inapperr(
            &AbstPath::from("some/path/to/somewhere"),
            "some terrible ugly error",
        );
        assert_eq!(
            err,
            InapplicableDelta(
                AbstPath::from("some/path/to/somewhere"),
                String::from("some terrible ugly error")
            )
        );
        assert_eq!(
            push_inapp("main")(err),
            InapplicableDelta(
                AbstPath::from("main/some/path/to/somewhere"),
                String::from("some terrible ugly error")
            )
        );
    }

    fn apply_at_endpoint() {
        let mut tree = FSTree::gen_from(|t| {
            t.add_dir("main", (580754122, 520360551), |t| {
                t.add_file("file", (1301408628, 476906800), "some content");
                t.add_symlink("symlink", (1180129474, 30754656), "some/path/to/somewhere");
                t.add_dir("dir", (580754122, 520360551), |t| {
                    t.add_file("file1", (580754122, 520360551), "some different content");
                });
            });
        });

        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file1",
                Some(FSNode::file(
                    (580754122, 520360551),
                    "some different content",
                )),
                None,
            );
            d.add_leaf(
                "file2",
                None,
                Some(FSNode::file(
                    (1202180268, 690072609),
                    "another different content",
                )),
            );
        });

        tree.clone()
            .apply_delta_at_endpoint(&delta, AbstPath::from("main/dir"))
            .unwrap();
        assert!(tree
            .clone()
            .apply_delta_at_endpoint(&delta, AbstPath::from("main/file"))
            .is_err());
        assert!(tree
            .clone()
            .apply_delta_at_endpoint(&delta, AbstPath::from("main/symlink"))
            .is_err());
        assert!(tree
            .clone()
            .apply_delta_at_endpoint(&delta, AbstPath::from("main/path-that-doesn't-exist"))
            .is_err());
        assert!(tree
            .apply_delta_at_endpoint(&delta, AbstPath::from("main"))
            .is_err());
    }

    fn apply() {
        // Leaf
        {
            let with_none = FSTree::empty();
            let with_old = FSTree::gen_from(|t| {
                t.add_file("file", (1346739082, 772415355), "old content");
            });
            let with_new = FSTree::gen_from(|t| {
                t.add_file("file", (1412647461, 938826506), "new content");
            });
            let with_other = FSTree::gen_from(|t| {
                t.add_file("file", (1294861033, 436961381), "other content");
            });

            let source = [
                (
                    None,
                    with_none.clone(),  // correct_source
                    with_old.clone(),   // wrong_source_1
                    with_other.clone(), // wrong_source_2
                ),
                (
                    Some(FSNode::file((1346739082, 772415355), "old content")),
                    with_old,          // correct_source
                    with_other,        // wrong_source_1
                    with_none.clone(), // wrong_source_2
                ),
            ];
            let target = [
                (None, with_none),
                (
                    Some(FSNode::file((1412647461, 938826506), "new content")),
                    with_new,
                ),
            ];

            for (source_node, correct_source, wrong_source_1, wrong_source_2) in source {
                for (target_node, target_tree) in target.clone() {
                    let delta = Delta::gen_from(|d| {
                        d.add_leaf("file", source_node.clone(), target_node.clone());
                    });

                    // correct source
                    let mut tree = correct_source.clone();
                    tree.apply_delta(&delta).unwrap();
                    assert_eq!(tree, target_tree);

                    // wrong source 1
                    let mut tree = wrong_source_1.clone();
                    assert!(tree.apply_delta(&delta).is_err());

                    // wrong source 1
                    let mut tree = wrong_source_2.clone();
                    assert!(tree.apply_delta(&delta).is_err());
                }
            }
        }

        // Branch
        {
            // Branch both on correct
            {
                let mut old_tree = FSTree::gen_from(|t| {
                    t.add_dir("dir", (504127312, 850809910), |t| {
                        t.add_file("file", (639031454, 844698469), "old content");
                    });
                });
                let new_tree = FSTree::gen_from(|t| {
                    t.add_dir("dir", (894310328, 379690596), |t| {
                        t.add_file("file", (1202109107, 840618676), "new content");
                    });
                });

                let delta = Delta::gen_from(|d| {
                    d.add_branch(
                        "dir",
                        ((504127312, 850809910), (894310328, 379690596)),
                        |d| {
                            d.add_leaf(
                                "file",
                                Some(FSNode::file((639031454, 844698469), "old content")),
                                Some(FSNode::file((1202109107, 840618676), "new content")),
                            );
                        },
                    );
                });

                old_tree.apply_delta(&delta).unwrap();
                assert_eq!(old_tree, new_tree);
            }

            // Branch mtime on wrong mtime
            {
                let mut tree = FSTree::gen_from(|t| {
                    t.add_dir("dir", (563726258, 169812194), |t| {
                        t.add_file("file", (818074359, 720231487), "qwertyuiop");
                    });
                });
                let delta = Delta::gen_from(|d| {
                    d.add_empty_branch("dir", ((1140615005, 566816009), (1151990024, 189224283)));
                });

                assert!(tree.apply_delta(&delta).is_err());
            }

            // Branch on wrong node
            {
                let delta = Delta::gen_from(|d| {
                    d.add_empty_branch("dir", ((772556155, 27466130), (772556155, 27466130)));
                });

                // on file
                let mut tree = FSTree::gen_from(|t| {
                    t.add_file(
                        "dir",
                        (772556155, 27466130),
                        "It doesn't matter what I write here",
                    );
                });
                assert!(tree.apply_delta(&delta).is_err());

                // on symlink
                let mut tree = FSTree::gen_from(|t| {
                    t.add_symlink("dir", (536938468, 588641777), "jus/imagine/this/is/a/path");
                });
                assert!(tree.apply_delta(&delta).is_err());

                // on missing
                assert!(FSTree::empty().apply_delta(&delta).is_err());
            }
        }
    }

    fn undo() {
        // Leaf
        {
            let with_none = FSTree::empty();
            let with_old = FSTree::gen_from(|t| {
                t.add_file("file", (835060797, 689306331), "old content");
            });
            let with_new = FSTree::gen_from(|t| {
                t.add_file("file", (835060797, 689306331), "new content");
            });
            let with_other = FSTree::gen_from(|t| {
                t.add_file("file", (1466699534, 795055847), "other content");
            });

            let source = [
                (None, with_none.clone()),
                (
                    Some(FSNode::file((835060797, 689306331), "old content")),
                    with_old,
                ),
            ];
            let target = [
                (
                    None,
                    with_none.clone(),  // correct_target
                    with_new.clone(),   // wrong_target_1
                    with_other.clone(), // wrong_target_2
                ),
                (
                    Some(FSNode::file((835060797, 689306331), "new content")),
                    with_new,   // correct_target
                    with_other, // wrong_target_1
                    with_none,  // wrong_target_2
                ),
            ];

            for (source_node, source_tree) in source {
                for (target_node, correct_target, wrong_target_1, wrong_target_2) in target.clone()
                {
                    let delta = Delta::gen_from(|d| {
                        d.add_leaf("file", source_node.clone(), target_node.clone());
                    });

                    // correct target
                    let mut tree = correct_target.clone();
                    tree.undo_delta(&delta).unwrap();
                    assert_eq!(tree, source_tree);

                    // wrong target 1
                    let mut tree = wrong_target_1.clone();
                    assert!(tree.undo_delta(&delta).is_err());

                    // wrong target 2
                    let mut tree = wrong_target_2.clone();
                    assert!(tree.undo_delta(&delta).is_err());
                }
            }
        }

        // Branch
        {
            // Branch both on correct
            {
                let old_tree = FSTree::gen_from(|t| {
                    t.add_dir("dir", (943474742, 242156167), |t| {
                        t.add_file("file", (576998097, 714596691), "boop beep boop");
                    });
                });
                let mut new_tree = FSTree::gen_from(|t| {
                    t.add_dir("dir", (522369369, 121597026), |t| {
                        t.add_file("file", (1387503744, 18260525), "ping pong bzzzz");
                    });
                });

                let delta = Delta::gen_from(|d| {
                    d.add_branch(
                        "dir",
                        ((943474742, 242156167), (522369369, 121597026)),
                        |d| {
                            d.add_leaf(
                                "file",
                                Some(FSNode::file((576998097, 714596691), "boop beep boop")),
                                Some(FSNode::file((1387503744, 18260525), "ping pong bzzzz")),
                            );
                        },
                    );
                });

                new_tree.undo_delta(&delta).unwrap();
                assert_eq!(old_tree, new_tree);
            }

            // Branch mtime on wrong mtime
            {
                let mut tree = FSTree::gen_from(|t| {
                    t.add_dir("dir", (1139146882, 934492139), |t| {
                        t.add_file("file", (818074359, 720231487), "asdfghjkl");
                    });
                });
                let delta = Delta::gen_from(|d| {
                    d.add_empty_branch("dir", ((931306566, 389992216), (1170146405, 763554716)));
                });
                assert!(tree.undo_delta(&delta).is_err());
            }

            // Branch on wrong node
            {
                let delta = Delta::gen_from(|d| {
                    d.add_empty_branch("dir", ((1111032689, 805260693), (1111032689, 805260693)));
                });

                // on file
                let mut tree = FSTree::gen_from(|t| {
                    t.add_file("dir", (1111032689, 805260693), "here is content");
                });
                assert!(tree.undo_delta(&delta).is_err());

                // on symlink
                let mut tree = FSTree::gen_from(|t| {
                    t.add_symlink("dir", (1111032689, 805260693), "path/that/leads/to");
                });
                assert!(tree.undo_delta(&delta).is_err());

                // on missing
                assert!(FSTree::empty().undo_delta(&delta).is_err());
            }
        }
    }
}
