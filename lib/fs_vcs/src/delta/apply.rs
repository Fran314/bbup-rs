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
    use super::{super::get_delta, inapperr, push_inapp, Delta, FSNode, FSTree, InapplicableDelta};
    use abst_fs::AbstPath;

    #[test]
    fn test_error() {
        let err = inapperr(
            &AbstPath::from("some/path/to/somewhere"),
            "some error message",
        );
        assert_eq!(
            err,
            InapplicableDelta(
                AbstPath::from("some/path/to/somewhere"),
                String::from("some error message")
            )
        );
        assert_eq!(
            push_inapp("main")(err),
            InapplicableDelta(
                AbstPath::from("main/some/path/to/somewhere"),
                String::from("some error message")
            )
        );
    }

    #[test]
    fn test_apply_undo() {
        let pre_fstree = FSTree::gen_from(|t| {
            t.add_file("untouched-file", (1664593215, 129813725), "content 0");
            t.add_symlink("untouched-symlink", (1664638936, 489785707), "path/to/0");
            t.add_dir("untouched-dir", (1664839418, 456172131), |t| {
                t.add_file("some-file", (1664659645, 469489956), "content 1");
                t.add_symlink("some-symlink", (1664685202, 499366060), "path/to/1");
                t.add_dir("some-dir", (1664820056, 176682064), |t| {
                    t.add_file("some-subfile", (1664730750, 77149506), "content 2");
                    t.add_symlink("some-subsymlink", (1664773722, 691287467), "path/to/2");
                    t.add_empty_dir("some-subdir", (1664807510, 362072320));
                });
            });

            t.add_file("removed-file", (1664844899, 952880867), "content 3");
            t.add_symlink("removed-symlink", (1664866516, 799949137), "path/to/3");
            t.add_dir("removed-dir", (1665080760, 125873632), |t| {
                t.add_file("some-file", (1664904983, 225354006), "content 4");
                t.add_symlink("some-symlink", (1664931292, 707636324), "path/to/4");
                t.add_dir("some-dir", (1665051015, 728013427), |t| {
                    t.add_file("some-subsfile", (1664969116, 618383875), "content 5");
                    t.add_symlink("some-subsymlink", (1665009784, 973406400), "path/to/5");
                    t.add_empty_dir("some-subdir", (1665020782, 554599289));
                });
            });

            t.add_file("mtime-edit-file", (1665128681, 479153113), "content 6");
            t.add_symlink("mtime-edit-symlink", (1665223326, 633517793), "path/to/6");
            t.add_dir("mtime-edit-dir", (1665231730, 757614747), |t| {
                t.add_file("some-file", (1665246903, 757311754), "content 7");
                t.add_symlink("some-symlink", (1665267994, 243823157), "path/to/7");
                t.add_dir("some-dir", (1665382767, 244824259), |t| {
                    t.add_file("some-subfile", (1665320210, 926569705), "content 8");
                    t.add_symlink("some-subsymlink", (1665331146, 202356737), "path/to/8");
                    t.add_empty_dir("some-subdir", (1665361211, 62943599));
                });
            });

            t.add_file("content-edit-file", (1665403797, 984813446), "content 9");
            t.add_symlink("content-edit-symlink", (1665492280, 294042651), "path/to/9");
            t.add_dir("content-edit-dir", (1665653391, 583942877), |t| {
                t.add_file("old-file", (1665537545, 274720731), "content 10");
                t.add_symlink("old-symlink", (1665578089, 400706450), "path/to/10");
                t.add_dir("old-dir", (1665616031, 855387955), |t| {
                    t.add_file("some-file", (1665593626, 191212804), "content 11");
                    t.add_symlink("some-symlink", (1665602011, 364167939), "path/to/11");
                    t.add_empty_dir("some-dir", (1665609609, 381366620));
                });
            });

            t.add_file("both-edit-file", (1665658948, 294056682), "content 12");
            t.add_symlink("both-edit-symlink", (1665706590, 498424292), "path/to/12");
            t.add_dir("both-edit-dir", (1665857459, 273562674), |t| {
                t.add_file("old-file", (1665721719, 759507069), "content 13");
                t.add_symlink("old-symlink", (1665742729, 864183276), "path/to/13");
                t.add_dir("old-dir", (1665823151, 430141738), |t| {
                    t.add_file("some-file", (1665753800, 479487453), "content 14");
                    t.add_symlink("some-symlink", (1665799314, 73687095), "path/to/14");
                    t.add_empty_dir("some-dir", (1665816185, 637073506));
                });
            });

            t.add_file("file-to-symlink", (1665878934, 122842597), "content 15");
            t.add_symlink("symlink-to-file", (1665925952, 816940720), "path/to/15");
            t.add_file("file-to-dir", (1665952861, 367324405), "content 16");
            t.add_dir("dir-to-file", (1666112742, 844333980), |t| {
                t.add_file("some-file", (1665980032, 483481851), "content 17");
                t.add_symlink("some-symlink", (1665989441, 197429024), "path/to/17");
                t.add_dir("some-dir", (1666091840, 265768979), |t| {
                    t.add_file("some-subfile", (1666003479, 80356802), "content 18");
                    t.add_symlink("some-subsymlink", (1666009206, 612314999), "path/to/18");
                    t.add_empty_dir("some-subdir", (1666057999, 808033458))
                });
            });
            t.add_symlink("symlink-to-dir", (1666150895, 596092504), "path/to/19");
            t.add_dir("dir-to-symlink", (1666619883, 311193088), |t| {
                t.add_file("some-file", (1666160237, 675128780), "content 20");
                t.add_symlink("some-symlink", (1666226534, 830436513), "path/to/20");
                t.add_dir("some-dir", (1666556719, 684833087), |t| {
                    t.add_file("some-subfile", (1666307759, 331079248), "content 21");
                    t.add_symlink("some-subsymlink", (1666367800, 117412925), "path/to/21");
                    t.add_empty_dir("some-subdir", (1666467991, 57155305));
                });
            });
        });
        let post_fstree = FSTree::gen_from(|t| {
            t.add_file("untouched-file", (1664593215, 129813725), "content 0");
            t.add_symlink("untouched-symlink", (1664638936, 489785707), "path/to/0");
            t.add_dir("untouched-dir", (1664839418, 456172131), |t| {
                t.add_file("some-file", (1664659645, 469489956), "content 1");
                t.add_symlink("some-symlink", (1664685202, 499366060), "path/to/1");
                t.add_dir("some-dir", (1664820056, 176682064), |t| {
                    t.add_file("some-subfile", (1664730750, 77149506), "content 2");
                    t.add_symlink("some-subsymlink", (1664773722, 691287467), "path/to/2");
                    t.add_empty_dir("some-subdir", (1664807510, 362072320));
                });
            });

            t.add_file("added-file", (1667291618, 49665399), "content 22");
            t.add_symlink("added-symlink", (1667299371, 392444127), "path/to/22");
            t.add_dir("added-dir", (1667458204, 617921196), |t| {
                t.add_file("some-file", (1667344231, 62151406), "content 23");
                t.add_symlink("some-symlink", (1667386471, 512939450), "path/to/23");
                t.add_dir("some-dir", (1667452610, 239738758), |t| {
                    t.add_file("some-subsfile", (1667413109, 643123620), "content 24");
                    t.add_symlink("some-subsymlink", (1667430861, 703560783), "path/to/24");
                    t.add_empty_dir("some-subdir", (1667436674, 904022684));
                });
            });

            t.add_file("mtime-edit-file", (1667491403, 52601873), "content 6");
            t.add_symlink("mtime-edit-symlink", (1667512489, 728838837), "path/to/6");
            t.add_dir("mtime-edit-dir", (1667527639, 27312686), |t| {
                t.add_file("some-file", (1665246903, 757311754), "content 7");
                t.add_symlink("some-symlink", (1665267994, 243823157), "path/to/7");
                t.add_dir("some-dir", (1665382767, 244824259), |t| {
                    t.add_file("some-subfile", (1665320210, 926569705), "content 8");
                    t.add_symlink("some-subsymlink", (1665331146, 202356737), "path/to/8");
                    t.add_empty_dir("some-subdir", (1665361211, 62943599));
                });
            });

            t.add_file("content-edit-file", (1665403797, 984813446), "content 25");
            t.add_symlink(
                "content-edit-symlink",
                (1665492280, 294042651),
                "path/to/25",
            );
            t.add_dir("content-edit-dir", (1665653391, 583942877), |t| {
                t.add_file("new-file", (1667565223, 544854425), "content 26");
                t.add_symlink("new-symlink", (1667606671, 756872113), "path/to/26");
                t.add_dir("new-dir", (1667690533, 790228724), |t| {
                    t.add_file("some-file", (1667654446, 52879214), "content 27");
                    t.add_symlink("some-symlink", (1667660746, 340510588), "path/to/27");
                    t.add_empty_dir("some-dir", (1667666855, 573555324));
                });
            });

            t.add_file("both-edit-file", (1667700076, 989692858), "content 28");
            t.add_symlink("both-edit-symlink", (1667744237, 161786498), "path/to/28");
            t.add_dir("both-edit-dir", (1667979024, 483039443), |t| {
                t.add_file("new-file", (1667786823, 846244395), "content 29");
                t.add_symlink("new-symlink", (1667827505, 675050268), "path/to/29");
                t.add_dir("new-dir", (1667971390, 870864659), |t| {
                    t.add_file("some-file", (1667868245, 278758645), "content 30");
                    t.add_symlink("some-symlink", (1667907662, 970681147), "path/to/30");
                    t.add_empty_dir("some-dir", (1667932481, 458833587));
                });
            });

            t.add_symlink("file-to-symlink", (1667989833, 799488495), "path/to/31");
            t.add_file("symlink-to-file", (1668019367, 534284745), "content 31");
            t.add_dir("file-to-dir", (1668255717, 149282922), |t| {
                t.add_file("some-file", (1668066544, 615520517), "content 32");
                t.add_symlink("some-symlink", (1668116001, 308689102), "path/to/32");
                t.add_dir("some-dir", (1668214742, 157637364), |t| {
                    t.add_file("some-subfile", (1668131352, 951864648), "content 33");
                    t.add_symlink("some-subsymlink", (1668149860, 566666057), "path/to/33");
                    t.add_empty_dir("some-subdir", (1668187711, 556826003));
                });
            });
            t.add_file("dir-to-file", (1668348923, 385859136), "content 34");
            t.add_dir("symlink-to-dir", (1668649280, 308757064), |t| {
                t.add_file("some-file", (1668452197, 126511533), "content 35");
                t.add_symlink("some-symlink", (1668491214, 884187985), "path/to/35");
                t.add_dir("some-dir", (1668612644, 635406011), |t| {
                    t.add_file("some-subfile", (1668531025, 526845175), "content 36");
                    t.add_symlink("some-subsymlink", (1668541084, 634088395), "path/to/36");
                    t.add_empty_dir("some-subdir", (1668566846, 601299229));
                });
            });
            t.add_symlink("dir-to-symlink", (1668676395, 805654992), "path/to/37");
        });

        let delta = get_delta(&pre_fstree, &post_fstree);

        let mut fstree_to_upgrade = pre_fstree.clone();
        fstree_to_upgrade.apply_delta(&delta).unwrap();
        assert_eq!(fstree_to_upgrade, post_fstree);

        let mut fstree_to_downgrade = post_fstree;
        fstree_to_downgrade.undo_delta(&delta).unwrap();
        assert_eq!(fstree_to_downgrade, pre_fstree);
    }

    #[test]
    fn test_apply_error() {
        todo!()
    }
    // #[test]
    // fn test_apply() {
    //     // Leaf
    //     {
    //         let with_none = FSTree::empty();
    //         let with_old = FSTree::gen_from(|t| {
    //             t.add_file("file", (1346739082, 772415355), "old content");
    //         });
    //         let with_new = FSTree::gen_from(|t| {
    //             t.add_file("file", (1412647461, 938826506), "new content");
    //         });
    //         let with_other = FSTree::gen_from(|t| {
    //             t.add_file("file", (1294861033, 436961381), "other content");
    //         });
    //
    //         let source = [
    //             (
    //                 None,
    //                 with_none.clone(),  // correct_source
    //                 with_old.clone(),   // wrong_source_1
    //                 with_other.clone(), // wrong_source_2
    //             ),
    //             (
    //                 Some(FSNode::file((1346739082, 772415355), "old content")),
    //                 with_old,          // correct_source
    //                 with_other,        // wrong_source_1
    //                 with_none.clone(), // wrong_source_2
    //             ),
    //         ];
    //         let target = [
    //             (None, with_none),
    //             (
    //                 Some(FSNode::file((1412647461, 938826506), "new content")),
    //                 with_new,
    //             ),
    //         ];
    //
    //         for (source_node, correct_source, wrong_source_1, wrong_source_2) in source {
    //             for (target_node, target_tree) in target.clone() {
    //                 let delta = Delta::gen_from(|d| {
    //                     d.add_leaf("file", source_node.clone(), target_node.clone());
    //                 });
    //
    //                 // correct source
    //                 let mut tree = correct_source.clone();
    //                 tree.apply_delta(&delta).unwrap();
    //                 assert_eq!(tree, target_tree);
    //
    //                 // wrong source 1
    //                 let mut tree = wrong_source_1.clone();
    //                 assert!(tree.apply_delta(&delta).is_err());
    //
    //                 // wrong source 1
    //                 let mut tree = wrong_source_2.clone();
    //                 assert!(tree.apply_delta(&delta).is_err());
    //             }
    //         }
    //     }
    //
    //     // Branch
    //     {
    //         // Branch both on correct
    //         {
    //             let mut old_tree = FSTree::gen_from(|t| {
    //                 t.add_dir("dir", (504127312, 850809910), |t| {
    //                     t.add_file("file", (639031454, 844698469), "old content");
    //                 });
    //             });
    //             let new_tree = FSTree::gen_from(|t| {
    //                 t.add_dir("dir", (894310328, 379690596), |t| {
    //                     t.add_file("file", (1202109107, 840618676), "new content");
    //                 });
    //             });
    //
    //             let delta = Delta::gen_from(|d| {
    //                 d.add_branch(
    //                     "dir",
    //                     ((504127312, 850809910), (894310328, 379690596)),
    //                     |d| {
    //                         d.add_leaf(
    //                             "file",
    //                             Some(FSNode::file((639031454, 844698469), "old content")),
    //                             Some(FSNode::file((1202109107, 840618676), "new content")),
    //                         );
    //                     },
    //                 );
    //             });
    //
    //             old_tree.apply_delta(&delta).unwrap();
    //             assert_eq!(old_tree, new_tree);
    //         }
    //
    //         // Branch mtime on wrong mtime
    //         {
    //             let mut tree = FSTree::gen_from(|t| {
    //                 t.add_dir("dir", (563726258, 169812194), |t| {
    //                     t.add_file("file", (818074359, 720231487), "qwertyuiop");
    //                 });
    //             });
    //             let delta = Delta::gen_from(|d| {
    //                 d.add_empty_branch("dir", ((1140615005, 566816009), (1151990024, 189224283)));
    //             });
    //
    //             assert!(tree.apply_delta(&delta).is_err());
    //         }
    //
    //         // Branch on wrong node
    //         {
    //             let delta = Delta::gen_from(|d| {
    //                 d.add_empty_branch("dir", ((772556155, 27466130), (772556155, 27466130)));
    //             });
    //
    //             // on file
    //             let mut tree = FSTree::gen_from(|t| {
    //                 t.add_file(
    //                     "dir",
    //                     (772556155, 27466130),
    //                     "It doesn't matter what I write here",
    //                 );
    //             });
    //             assert!(tree.apply_delta(&delta).is_err());
    //
    //             // on symlink
    //             let mut tree = FSTree::gen_from(|t| {
    //                 t.add_symlink("dir", (536938468, 588641777), "jus/imagine/this/is/a/path");
    //             });
    //             assert!(tree.apply_delta(&delta).is_err());
    //
    //             // on missing
    //             assert!(FSTree::empty().apply_delta(&delta).is_err());
    //         }
    //     }
    // }
    //
    // fn undo() {
    //     // Leaf
    //     {
    //         let with_none = FSTree::empty();
    //         let with_old = FSTree::gen_from(|t| {
    //             t.add_file("file", (835060797, 689306331), "old content");
    //         });
    //         let with_new = FSTree::gen_from(|t| {
    //             t.add_file("file", (835060797, 689306331), "new content");
    //         });
    //         let with_other = FSTree::gen_from(|t| {
    //             t.add_file("file", (1466699534, 795055847), "other content");
    //         });
    //
    //         let source = [
    //             (None, with_none.clone()),
    //             (
    //                 Some(FSNode::file((835060797, 689306331), "old content")),
    //                 with_old,
    //             ),
    //         ];
    //         let target = [
    //             (
    //                 None,
    //                 with_none.clone(),  // correct_target
    //                 with_new.clone(),   // wrong_target_1
    //                 with_other.clone(), // wrong_target_2
    //             ),
    //             (
    //                 Some(FSNode::file((835060797, 689306331), "new content")),
    //                 with_new,   // correct_target
    //                 with_other, // wrong_target_1
    //                 with_none,  // wrong_target_2
    //             ),
    //         ];
    //
    //         for (source_node, source_tree) in source {
    //             for (target_node, correct_target, wrong_target_1, wrong_target_2) in target.clone()
    //             {
    //                 let delta = Delta::gen_from(|d| {
    //                     d.add_leaf("file", source_node.clone(), target_node.clone());
    //                 });
    //
    //                 // correct target
    //                 let mut tree = correct_target.clone();
    //                 tree.undo_delta(&delta).unwrap();
    //                 assert_eq!(tree, source_tree);
    //
    //                 // wrong target 1
    //                 let mut tree = wrong_target_1.clone();
    //                 assert!(tree.undo_delta(&delta).is_err());
    //
    //                 // wrong target 2
    //                 let mut tree = wrong_target_2.clone();
    //                 assert!(tree.undo_delta(&delta).is_err());
    //             }
    //         }
    //     }
    //
    //     // Branch
    //     {
    //         // Branch both on correct
    //         {
    //             let old_tree = FSTree::gen_from(|t| {
    //                 t.add_dir("dir", (943474742, 242156167), |t| {
    //                     t.add_file("file", (576998097, 714596691), "boop beep boop");
    //                 });
    //             });
    //             let mut new_tree = FSTree::gen_from(|t| {
    //                 t.add_dir("dir", (522369369, 121597026), |t| {
    //                     t.add_file("file", (1387503744, 18260525), "ping pong bzzzz");
    //                 });
    //             });
    //
    //             let delta = Delta::gen_from(|d| {
    //                 d.add_branch(
    //                     "dir",
    //                     ((943474742, 242156167), (522369369, 121597026)),
    //                     |d| {
    //                         d.add_leaf(
    //                             "file",
    //                             Some(FSNode::file((576998097, 714596691), "boop beep boop")),
    //                             Some(FSNode::file((1387503744, 18260525), "ping pong bzzzz")),
    //                         );
    //                     },
    //                 );
    //             });
    //
    //             new_tree.undo_delta(&delta).unwrap();
    //             assert_eq!(old_tree, new_tree);
    //         }
    //
    //         // Branch mtime on wrong mtime
    //         {
    //             let mut tree = FSTree::gen_from(|t| {
    //                 t.add_dir("dir", (1139146882, 934492139), |t| {
    //                     t.add_file("file", (818074359, 720231487), "asdfghjkl");
    //                 });
    //             });
    //             let delta = Delta::gen_from(|d| {
    //                 d.add_empty_branch("dir", ((931306566, 389992216), (1170146405, 763554716)));
    //             });
    //             assert!(tree.undo_delta(&delta).is_err());
    //         }
    //
    //         // Branch on wrong node
    //         {
    //             let delta = Delta::gen_from(|d| {
    //                 d.add_empty_branch("dir", ((1111032689, 805260693), (1111032689, 805260693)));
    //             });
    //
    //             // on file
    //             let mut tree = FSTree::gen_from(|t| {
    //                 t.add_file("dir", (1111032689, 805260693), "here is content");
    //             });
    //             assert!(tree.undo_delta(&delta).is_err());
    //
    //             // on symlink
    //             let mut tree = FSTree::gen_from(|t| {
    //                 t.add_symlink("dir", (1111032689, 805260693), "path/that/leads/to");
    //             });
    //             assert!(tree.undo_delta(&delta).is_err());
    //
    //             // on missing
    //             assert!(FSTree::empty().undo_delta(&delta).is_err());
    //         }
    //     }
    // }
}
