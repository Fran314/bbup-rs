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
    pub fn apply_delta(&mut self, deltatree: &Delta) -> Result<(), InapplicableDelta> {
        use std::collections::hash_map::Entry::{Occupied, Vacant};
        use DeltaNode::{Branch, Leaf};
        for (name, child) in deltatree {
            match child {
                Leaf(None, None) => {
                    // This is an unshaken node, maybe I should say something about it
                    match self.entry(name.clone()) {
                        Vacant(_) => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                &AbstPath::single(name),
                                "delta claims this node is None, but it exists in tree",
                            ));
                        }
                    }
                }
                Leaf(Some(pre), None) => match self.entry(name.clone()) {
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
                Leaf(None, Some(post)) => match self.entry(name.clone()) {
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
                Leaf(Some(pre), Some(post)) => match self.entry(name.clone()) {
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
                    match self.entry(name.clone()) {
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
    pub fn undo_delta(&mut self, deltatree: &Delta) -> Result<(), InapplicableDelta> {
        use std::collections::hash_map::Entry::*;
        use DeltaNode::*;
        for (name, child) in deltatree {
            match child {
                Leaf(None, None) => {
                    // This is an unshaken node, maybe I should say something about it
                    match self.entry(name.clone()) {
                        Vacant(_) => {}
                        Occupied(_) => {
                            return Err(inapperr(
                                &AbstPath::single(name),
                                "delta claims this node is None, but it exists in tree",
                            ));
                        }
                    }
                }
                Leaf(Some(pre), None) => match self.entry(name.clone()) {
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
                Leaf(None, Some(post)) => match self.entry(name.clone()) {
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
                Leaf(Some(pre), Some(post)) => match self.entry(name.clone()) {
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
                    match self.entry(name.clone()) {
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
    fn test_apply_remove_nonexisting() {
        // removing non-existing file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("file-0", (1664611053, 991844947), "content 0");
            t.add_file("file-1", (1664640986, 701498151), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file-2",
                Some(FSNode::file((1664659510, 164704015), "content 2")),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing non-existing symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink-0", (1664687905, 647574844), "path/to/0");
            t.add_symlink("symlink-1", (1664715532, 552336991), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink-2",
                Some(FSNode::symlink((1664749226, 345431361), "path/to/2")),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing non-existing dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir-0", (1664926494, 906084011), |t| {
                t.add_file("some-file", (1664779985, 368405972), "content 0");
                t.add_symlink("some-symlink", (1664863659, 405600251), "path/to/0");
                t.add_empty_dir("some-dir", (1664909213, 280554341));
            });
            t.add_dir("dir-1", (1665066224, 798681145), |t| {
                t.add_file("some-file", (1664966299, 908260663), "content 1");
                t.add_symlink("some-symlink", (1665015610, 117016415), "path/to/1");
                t.add_empty_dir("some-dir", (1665056745, 23088061));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "dir-2",
                Some(FSNode::dir((1665159268, 305917966), |t| {
                    t.add_file("some-file", (1665087028, 538817223), "content 2");
                    t.add_symlink("some-symlink", (1665103199, 769350299), "path/to/2");
                    t.add_empty_dir("some-dir", (1665137420, 391289932));
                })),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_remove_mismatching() {
        // removing mismatching file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1664599139, 156636108), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                Some(FSNode::file((1664619975, 697985282), "content 1")),
                None,
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing mismatching symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1664644674, 107397496), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                Some(FSNode::symlink((1664685021, 263452999), "path/to/1")),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing mismatching dir
        {
            let mut pre_fstree = FSTree::gen_from(|t| {
                t.add_dir("dir", (1664771986, 686464781), |t| {
                    t.add_file("file", (1664740586, 418704042), "content 0");
                });
            });
            let delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "dir",
                    Some(FSNode::dir((1664771986, 686464781), |t| {
                        t.add_file("file", (1664808494, 276146150), "content 1");
                    })),
                    None,
                )
            });
            assert!(pre_fstree.apply_delta(&delta).is_err());

            let mut pre_fstree = FSTree::gen_from(|t| {
                t.add_empty_dir("dir", (1664857942, 516069226));
            });
            let delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "dir",
                    Some(FSNode::empty_dir((1664893740, 213730497))),
                    None,
                );
            });
            assert!(pre_fstree.apply_delta(&delta).is_err());
        }
    }

    #[test]
    fn test_apply_remove_wrong_object() {
        // removing file but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664913104, 200357169), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1664935144, 194659993), "content 0")),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing file but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665050110, 770707436), |t| {
                t.add_file("some-file", (1664947202, 540226819), "content 0");
                t.add_symlink("some-symlink", (1664962482, 345146125), "path/to/0");
                t.add_empty_dir("some-dir", (1665011320, 760337327));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665065509, 631283713), "content 1")),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing symlink but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665105694, 543227806), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665118892, 375127706), "path/to/1")),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing symlink but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665245682, 454657088), |t| {
                t.add_file("some-file", (1665165996, 423698201), "content 0");
                t.add_symlink("some-symlink", (1665196999, 346794716), "path/to/0");
                t.add_empty_dir("some-dir", (1665233827, 519546045));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665251468, 811163638), "path/to/1")),
                None,
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing dir but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665287968, 75165294), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665398969, 735323982), |t| {
                    t.add_file("some-file", (1665337316, 903304884), "content 1");
                    t.add_symlink("some-symlink", (1665347426, 976023713), "path/to/1");
                    t.add_empty_dir("some-dir", (1665373964, 782167244));
                })),
                None,
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // removing dir but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1665424516, 128214197), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665516445, 838968877), |t| {
                    t.add_file("some-file", (1665439020, 15738585), "content 1");
                    t.add_symlink("some-symlink", (1665466267, 353121218), "path/to/1");
                    t.add_empty_dir("some-dir", (1665487531, 926797214));
                })),
                None,
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_add_already_existing() {
        // adding alredy-existing file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1665176196, 587859193), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                None,
                Some(FSNode::file((1665214576, 572012016), "content 1")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // adding already-existing symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1665249364, 65256143), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                None,
                Some(FSNode::symlink((1665271579, 998007784), "path/to/1")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // adding already-existing dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1665376245, 239052552), |t| {
                t.add_file("some-file", (1665300924, 78630849), "content 0");
                t.add_symlink("some-symlink", (1665326519, 178145032), "path/to/0");
                t.add_empty_dir("some-dir", (1665349492, 826912630));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "dir",
                None,
                Some(FSNode::dir((1665483427, 763302169), |t| {
                    t.add_file("some-file", (1665399405, 197728937), "content 1");
                    t.add_symlink("some-symlink", (1665446756, 403502476), "path/to/1");
                    t.add_empty_dir("some-dir", (1665457279, 329361486));
                })),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_edit_nonexisting() {
        // editing non-existing file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("file-0", (1665492779, 557878123), "content 0");
            t.add_file("file-1", (1665505443, 267586144), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file-2",
                Some(FSNode::file((1665537979, 635137994), "content 2")),
                Some(FSNode::file((1665553855, 931129047), "content 3")),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing non-existing symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink-0", (1665591464, 114652016), "path/to/0");
            t.add_symlink("symlink-1", (1665611377, 89326268), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink-2",
                Some(FSNode::symlink((1665646554, 65848117), "path/to/2")),
                Some(FSNode::symlink((1665652002, 569035141), "path/to/3")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing non-existing dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir-0", (1665803693, 795401529), |t| {
                t.add_file("some-file", (1665686370, 816036591), "content 0");
                t.add_symlink("some-symlink", (1665733953, 406950861), "path/to/0");
                t.add_empty_dir("some-dir", (1665757779, 530203302));
            });
            t.add_dir("dir-0", (1665897256, 956132480), |t| {
                t.add_file("some-file", (1665816972, 350751600), "content 1");
                t.add_symlink("some-symlink", (1665843198, 760010293), "path/to/1");
                t.add_empty_dir("some-dir", (1665865444, 293250629));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_branch(
                "dir-2",
                ((1665952904, 290573249), (1666030918, 776509832)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1665945535, 128950999), "content 2")),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1666001706, 282989530), "content 3")),
                    );
                },
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_edit_mismatching() {
        // editing mismatching file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1666049438, 440360851), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                Some(FSNode::file((1666049438, 440360851), "content 1")),
                Some(FSNode::file((1666099423, 557038785), "content 2")),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing mismatching symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1666143561, 463774965), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                Some(FSNode::symlink((1666150735, 189490071), "path/to/1")),
                Some(FSNode::symlink((1666158405, 595181891), "path/to/2")),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing mismatching dir
        {
            let mut pre_fstree = FSTree::gen_from(|t| {
                t.add_dir("dir", (1666204799, 644014366), |t| {
                    t.add_file("file", (1666166295, 563380163), "content 0");
                });
            });
            let delta = Delta::gen_from(|d| {
                d.add_branch(
                    "dir",
                    ((1666204799, 644014366), (1666284281, 893730352)),
                    |d| {
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1666216552, 953819364), "content 1")),
                            Some(FSNode::file((1666254771, 780720460), "content 2")),
                        )
                    },
                );
            });
            assert!(pre_fstree.apply_delta(&delta).is_err());

            let mut pre_fstree = FSTree::gen_from(|t| {
                t.add_empty_dir("dir", (1666309556, 494846337));
            });
            let delta = Delta::gen_from(|d| {
                d.add_empty_branch("dir", ((1666317790, 881322421), (1666326009, 407586031)));
            });
            assert!(pre_fstree.apply_delta(&delta).is_err());
        }
    }

    #[test]
    fn test_apply_edit_wrong_object() {
        // editing file but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1665521814, 361362753), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665557448, 433184586), "content 1")),
                Some(FSNode::file((1665591861, 496860274), "content 2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing file but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665714456, 921517643), |t| {
                t.add_file("some-file", (1665629918, 748815157), "content 0");
                t.add_symlink("some-symlink", (1665646889, 680207647), "path/to/0");
                t.add_empty_dir("some-dir", (1665671079, 418483515));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665749542, 493776715), "content 1")),
                Some(FSNode::file((1665773476, 126986370), "content 2")),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing symlink but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665793485, 252381075), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665822311, 366487318), "path/to/1")),
                Some(FSNode::symlink((1665851905, 551315877), "path/to/2")),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing symlink but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665979811, 781520250), |t| {
                t.add_file("some-file", (1665863625, 368006735), "content 0");
                t.add_symlink("some-symlink", (1665901150, 869201833), "path/to/0");
                t.add_empty_dir("some-dir", (1665932686, 971607668));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665987669, 495661603), "path/to/1")),
                Some(FSNode::symlink((1665996067, 478657676), "path/to/2")),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing dir but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1666042297, 968081641), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_branch(
                "object",
                ((1666099964, 953749425), (1666099964, 953749425)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1666087020, 471242920), "content 1")),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1666099964, 953749425), "content 2")),
                    );
                },
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // editing dir but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1666209106, 3646732), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_branch(
                "object",
                ((1666259672, 116816453), (1666332978, 637258610)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1666230133, 997350604), "content 1")),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1666301106, 140598780), "content 2")),
                    );
                },
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_transmute_nonexisting() {
        // transmute nonexisting file to symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object-0", (1664619019, 697849805), "content 0");
            t.add_file("object-1", (1664633143, 755697863), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::file((1664674616, 573912655), "content 2")),
                Some(FSNode::symlink((1664720070, 747441227), "path/to/3")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute nonexisting file to dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object-0", (1664766639, 854652024), "content 0");
            t.add_file("object-1", (1664811807, 334810677), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::file((1664828655, 324956590), "content 2")),
                Some(FSNode::dir((1664982607, 8627944), |t| {
                    t.add_file("some-file", (1664862410, 986719007), "content 3");
                    t.add_symlink("some-symlink", (1664888975, 704504785), "path/to/3");
                    t.add_empty_dir("some-dir", (1664936790, 497348095));
                })),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute nonexisting symlink to file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object-0", (1665011389, 345426378), "path/to/0");
            t.add_symlink("object-1", (1665023749, 695865125), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::symlink((1665035883, 269660467), "path/to/2")),
                Some(FSNode::file((1665046515, 676597233), "content 3")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute nonexisting symlink to dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object-0", (1665081784, 539931040), "path/to/0");
            t.add_symlink("object-1", (1665121679, 205508258), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::symlink((1665131947, 634892894), "path/to/2")),
                Some(FSNode::dir((1665193461, 50569743), |t| {
                    t.add_file("some-file", (1665164779, 339344016), "content 3");
                    t.add_symlink("some-symlink", (1665170012, 457744421), "path/to/3");
                    t.add_empty_dir("some-dir", (1665183761, 992398962));
                })),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute nonexisting dir to file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object-0", (1665226643, 621181694), |t| {
                t.add_file("some-file", (1665184929, 369311020), "content 0");
                t.add_symlink("some-symlink", (1665197113, 716903067), "path/to/0");
                t.add_empty_dir("some-dir", (1665209413, 678611758));
            });
            t.add_dir("object-1", (1665344366, 242926702), |t| {
                t.add_file("some-file", (1665250516, 692347657), "content 1");
                t.add_symlink("some-symlink", (1665281508, 902872499), "path/to/1");
                t.add_empty_dir("some-dir", (1665317513, 520288957));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::dir((1665433500, 356535968), |t| {
                    t.add_file("some-file", (1665351236, 396805122), "content 2");
                    t.add_symlink("some-symlink", (1665379044, 243150804), "path/to/2");
                    t.add_empty_dir("some-dir", (1665408430, 602298397));
                })),
                Some(FSNode::file((1665481636, 635638016), "content 3")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute nonexisting dir to symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object-0", (1665589329, 756294028), |t| {
                t.add_file("some-file", (1665508406, 45617910), "content 0");
                t.add_symlink("some-symlink", (1665532866, 189059067), "path/to/0");
                t.add_empty_dir("some-dir", (1665541627, 916689825));
            });
            t.add_dir("object-1", (1665694450, 246671580), |t| {
                t.add_file("some-file", (1665620395, 523262183), "content 1");
                t.add_symlink("some-symlink", (1665628218, 527908330), "path/to/1");
                t.add_empty_dir("some-dir", (1665673678, 881802329));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::dir((1665816611, 424928715), |t| {
                    t.add_file("some-file", (1665712674, 690993062), "content 2");
                    t.add_symlink("some-symlink", (1665753194, 602350633), "path/to/2");
                    t.add_empty_dir("some-dir", (1665779986, 107799935));
                })),
                Some(FSNode::symlink((1665835485, 647231622), "path/to/3")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_transmute_mismatching() {
        // transmute mismatching file to symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1664673025, 531984009), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1664693630, 967328151), "content 1")),
                Some(FSNode::symlink((1664716158, 54721864), "path/to/2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute mismatching file to dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1664723688, 221652185), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1664749110, 723918455), "content 1")),
                Some(FSNode::dir((1664842432, 523939334), |t| {
                    t.add_file("some-file", (1664774413, 876453884), "content 2");
                    t.add_symlink("some-symlink", (1664811456, 46886211), "path/to/2");
                    t.add_empty_dir("some-dir", (1664820140, 444887751));
                })),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute mismatching symlink to file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664855978, 653241183), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1664872647, 262354707), "path/to/1")),
                Some(FSNode::file((1664908075, 182949859), "content 2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute mismatching symlink to dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664928206, 92380148), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1664948132, 673852066), "path/to/1")),
                Some(FSNode::dir((1665075137, 390334098), |t| {
                    t.add_file("some-file", (1664986001, 30472811), "content 2");
                    t.add_symlink("some-symlink", (1665022870, 30557425), "path/to/2");
                    t.add_empty_dir("some-dir", (1665049626, 866335883));
                })),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute mismatching dir to file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665138466, 99312617), |t| {
                t.add_file("file", (1665075137, 390334098), "content 0");
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665209633, 937534252), |t| {
                    t.add_file("file", (1665181295, 795915315), "content 1");
                })),
                Some(FSNode::file((1665223813, 995319051), "content 2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute mismatching dir to symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665230757, 843925147), |t| {
                t.add_file("file", (1665273003, 266087346), "content 0");
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665354701, 594926757), |t| {
                    t.add_file("file", (1665311511, 243935669), "content 1");
                })),
                Some(FSNode::symlink((1665364310, 989826193), "path/to/2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_transmute_wrong_object() {
        // transmute file to symlink but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664620330, 257350981), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1664632959, 247161434), "content 1")),
                Some(FSNode::symlink((1664620330, 257350981), "path/to/0")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute file to symlink but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1664796698, 908140288), |t| {
                t.add_file("some-file", (1664729218, 263479937), "content 0");
                t.add_symlink("some-symlink", (1664751023, 276999578), "path/to/0");
                t.add_empty_dir("some-dir", (1664790041, 31774295));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1664844533, 550686387), "content 1")),
                Some(FSNode::symlink((1664866044, 397658247), "path/to/2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute file to dir but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664887542, 251836758), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1664926100, 338547672), "content 1")),
                Some(FSNode::dir((1665068699, 836816921), |t| {
                    t.add_file("some-file", (1664943817, 921517573), "content 2");
                    t.add_symlink("some-symlink", (1664984088, 456662782), "path/to/2");
                    t.add_empty_dir("some-dir", (1665029724, 672998058));
                })),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute file to dir but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665174421, 78794134), |t| {
                t.add_file("some-file", (1665086746, 886785330), "content 0");
                t.add_symlink("some-symlink", (1665134082, 217256394), "path/to/0");
                t.add_empty_dir("some-dir", (1665161109, 217172940));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665180363, 145300634), "content 1")),
                Some(FSNode::dir((1665174421, 78794134), |t| {
                    t.add_file("some-file", (1665086746, 886785330), "content 0");
                    t.add_symlink("some-symlink", (1665134082, 217256394), "path/to/0");
                    t.add_empty_dir("some-dir", (1665161109, 217172940));
                })),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute symlink to file but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665198682, 501454596), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665213774, 548259573), "path/to/1")),
                Some(FSNode::file((1665198682, 501454596), "content 0")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute symlink to file but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665315693, 217934808), |t| {
                t.add_file("some-file", (1665223864, 956465025), "content 0");
                t.add_symlink("some-symlink", (1665229423, 645588163), "path/to/0");
                t.add_empty_dir("some-dir", (1665268564, 587478173));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665332544, 163082273), "path/to/2")),
                Some(FSNode::file((1665353138, 252112606), "content 3")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute symlink to dir but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665373319, 620241392), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665405981, 77403437), "path/to/1")),
                Some(FSNode::dir((1665510003, 114881429), |t| {
                    t.add_file("some-file", (1665418776, 192974597), "content 2");
                    t.add_symlink("some-symlink", (1665457018, 579560393), "path/to/2");
                    t.add_empty_dir("some-dir", (1665501060, 370616301));
                })),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute symlink to dir but is dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665609246, 799056263), |t| {
                t.add_file("some-file", (1665552003, 276174523), "content 0");
                t.add_symlink("some-symlink", (1665563610, 751739194), "path/to/0");
                t.add_empty_dir("some-dir", (1665582808, 527492373));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665640339, 627094922), "path/to/1")),
                Some(FSNode::dir((1665609246, 799056263), |t| {
                    t.add_file("some-file", (1665552003, 276174523), "content 0");
                    t.add_symlink("some-symlink", (1665563610, 751739194), "path/to/0");
                    t.add_empty_dir("some-dir", (1665582808, 527492373));
                })),
            )
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute dir to file but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665650138, 384255228), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665768962, 574825905), |t| {
                    t.add_file("some-file", (1665698722, 789446634), "content 1");
                    t.add_symlink("some-symlink", (1665727059, 5245952), "path/to/1");
                    t.add_empty_dir("some-dir", (1665748673, 992499392));
                })),
                Some(FSNode::file((1665650138, 384255228), "content 0")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute dir to file but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1665777233, 452200534), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665881537, 816224265), |t| {
                    t.add_file("some-file", (1665816010, 998497452), "content 1");
                    t.add_symlink("some-symlink", (1665836879, 274755508), "path/to/1");
                    t.add_empty_dir("some-dir", (1665876392, 388955341));
                })),
                Some(FSNode::file((1665899919, 843765035), "content 2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute dir to symlink but is file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665929611, 833564144), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1666056340, 104021528), |t| {
                    t.add_file("some-file", (1665957778, 552659398), "content 1");
                    t.add_symlink("some-symlink", (1666005439, 3804866), "path/to/1");
                    t.add_empty_dir("some-dir", (1666033124, 159460868));
                })),
                Some(FSNode::symlink((1665768962, 574825905), "path/to/2")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // transmute dir to symlink but is symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1666134255, 165823697), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1666239427, 623712152), |t| {
                    t.add_file("some-file", (1666169326, 413069838), "content 1");
                    t.add_symlink("some-symlink", (1666197890, 103691698), "path/to/1");
                    t.add_empty_dir("some-dir", (1666227697, 320102340));
                })),
                Some(FSNode::file((1666134255, 165823697), "path/to/0")),
            );
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_add_nonexisting() {
        // undoing add non-existing file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("file-0", (1664611053, 991844947), "content 0");
            t.add_file("file-1", (1664640986, 701498151), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file-2",
                None,
                Some(FSNode::file((1664659510, 164704015), "content 2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add non-existing symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink-0", (1664687905, 647574844), "path/to/0");
            t.add_symlink("symlink-1", (1664715532, 552336991), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink-2",
                None,
                Some(FSNode::symlink((1664749226, 345431361), "path/to/2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add non-existing dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir-0", (1664926494, 906084011), |t| {
                t.add_file("some-file", (1664779985, 368405972), "content 0");
                t.add_symlink("some-symlink", (1664863659, 405600251), "path/to/0");
                t.add_empty_dir("some-dir", (1664909213, 280554341));
            });
            t.add_dir("dir-1", (1665066224, 798681145), |t| {
                t.add_file("some-file", (1664966299, 908260663), "content 1");
                t.add_symlink("some-symlink", (1665015610, 117016415), "path/to/1");
                t.add_empty_dir("some-dir", (1665056745, 23088061));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "dir-2",
                None,
                Some(FSNode::dir((1665159268, 305917966), |t| {
                    t.add_file("some-file", (1665087028, 538817223), "content 2");
                    t.add_symlink("some-symlink", (1665103199, 769350299), "path/to/2");
                    t.add_empty_dir("some-dir", (1665137420, 391289932));
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_add_mismatching() {
        // undoing add mismatching file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1664599139, 156636108), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                None,
                Some(FSNode::file((1664619975, 697985282), "content 1")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add mismatching symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1664644674, 107397496), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                None,
                Some(FSNode::symlink((1664685021, 263452999), "path/to/1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add mismatching dir
        {
            let mut post_fstree = FSTree::gen_from(|t| {
                t.add_dir("dir", (1664771986, 686464781), |t| {
                    t.add_file("file", (1664740586, 418704042), "content 0");
                });
            });
            let delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "dir",
                    None,
                    Some(FSNode::dir((1664771986, 686464781), |t| {
                        t.add_file("file", (1664808494, 276146150), "content 1");
                    })),
                )
            });
            assert!(post_fstree.undo_delta(&delta).is_err());

            let mut post_fstree = FSTree::gen_from(|t| {
                t.add_empty_dir("dir", (1664857942, 516069226));
            });
            let delta = Delta::gen_from(|d| {
                d.add_leaf(
                    "dir",
                    None,
                    Some(FSNode::empty_dir((1664893740, 213730497))),
                );
            });
            assert!(post_fstree.undo_delta(&delta).is_err());
        }
    }

    #[test]
    fn test_undo_add_wrong_object() {
        // undoing add file but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664913104, 200357169), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                None,
                Some(FSNode::file((1664935144, 194659993), "content 0")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add file but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665050110, 770707436), |t| {
                t.add_file("some-file", (1664947202, 540226819), "content 0");
                t.add_symlink("some-symlink", (1664962482, 345146125), "path/to/0");
                t.add_empty_dir("some-dir", (1665011320, 760337327));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                None,
                Some(FSNode::file((1665065509, 631283713), "content 1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add symlink but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665105694, 543227806), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                None,
                Some(FSNode::symlink((1665118892, 375127706), "path/to/1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add symlink but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665245682, 454657088), |t| {
                t.add_file("some-file", (1665165996, 423698201), "content 0");
                t.add_symlink("some-symlink", (1665196999, 346794716), "path/to/0");
                t.add_empty_dir("some-dir", (1665233827, 519546045));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                None,
                Some(FSNode::symlink((1665251468, 811163638), "path/to/1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add dir but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665287968, 75165294), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                None,
                Some(FSNode::dir((1665398969, 735323982), |t| {
                    t.add_file("some-file", (1665337316, 903304884), "content 1");
                    t.add_symlink("some-symlink", (1665347426, 976023713), "path/to/1");
                    t.add_empty_dir("some-dir", (1665373964, 782167244));
                })),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing add dir but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1665424516, 128214197), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                None,
                Some(FSNode::dir((1665516445, 838968877), |t| {
                    t.add_file("some-file", (1665439020, 15738585), "content 1");
                    t.add_symlink("some-symlink", (1665466267, 353121218), "path/to/1");
                    t.add_empty_dir("some-dir", (1665487531, 926797214));
                })),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_remove_existing() {
        // undoing remove existing file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1665176196, 587859193), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                Some(FSNode::file((1665214576, 572012016), "content 1")),
                None,
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing remove existing symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1665249364, 65256143), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                Some(FSNode::symlink((1665271579, 998007784), "path/to/1")),
                None,
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing remove existing dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1665376245, 239052552), |t| {
                t.add_file("some-file", (1665300924, 78630849), "content 0");
                t.add_symlink("some-symlink", (1665326519, 178145032), "path/to/0");
                t.add_empty_dir("some-dir", (1665349492, 826912630));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "dir",
                Some(FSNode::dir((1665483427, 763302169), |t| {
                    t.add_file("some-file", (1665399405, 197728937), "content 1");
                    t.add_symlink("some-symlink", (1665446756, 403502476), "path/to/1");
                    t.add_empty_dir("some-dir", (1665457279, 329361486));
                })),
                None,
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_edit_nonexisting() {
        // undoing edit non-existing file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("file-0", (1665492779, 557878123), "content 0");
            t.add_file("file-1", (1665505443, 267586144), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file-2",
                Some(FSNode::file((1665537979, 635137994), "content 2")),
                Some(FSNode::file((1665553855, 931129047), "content 3")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit non-existing symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink-0", (1665591464, 114652016), "path/to/0");
            t.add_symlink("symlink-1", (1665611377, 89326268), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink-2",
                Some(FSNode::symlink((1665646554, 65848117), "path/to/2")),
                Some(FSNode::symlink((1665652002, 569035141), "path/to/3")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit non-existing dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir-0", (1665803693, 795401529), |t| {
                t.add_file("some-file", (1665686370, 816036591), "content 0");
                t.add_symlink("some-symlink", (1665733953, 406950861), "path/to/0");
                t.add_empty_dir("some-dir", (1665757779, 530203302));
            });
            t.add_dir("dir-0", (1665897256, 956132480), |t| {
                t.add_file("some-file", (1665816972, 350751600), "content 1");
                t.add_symlink("some-symlink", (1665843198, 760010293), "path/to/1");
                t.add_empty_dir("some-dir", (1665865444, 293250629));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_branch(
                "dir-2",
                ((1665952904, 290573249), (1666030918, 776509832)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1665945535, 128950999), "content 2")),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1666001706, 282989530), "content 3")),
                    );
                },
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_edit_mismatching() {
        // undoing edit mismatching file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1666049438, 440360851), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                Some(FSNode::file((1666049438, 440360851), "content 1")),
                Some(FSNode::file((1666099423, 557038785), "content 2")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit mismatching symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1666143561, 463774965), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                Some(FSNode::symlink((1666150735, 189490071), "path/to/1")),
                Some(FSNode::symlink((1666158405, 595181891), "path/to/2")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit mismatching dir
        {
            let mut post_fstree = FSTree::gen_from(|t| {
                t.add_dir("dir", (1666204799, 644014366), |t| {
                    t.add_file("file", (1666166295, 563380163), "content 0");
                });
            });
            let delta = Delta::gen_from(|d| {
                d.add_branch(
                    "dir",
                    ((1666204799, 644014366), (1666284281, 893730352)),
                    |d| {
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1666216552, 953819364), "content 1")),
                            Some(FSNode::file((1666254771, 780720460), "content 2")),
                        )
                    },
                );
            });
            assert!(post_fstree.undo_delta(&delta).is_err());

            let mut post_fstree = FSTree::gen_from(|t| {
                t.add_empty_dir("dir", (1666309556, 494846337));
            });
            let delta = Delta::gen_from(|d| {
                d.add_empty_branch("dir", ((1666317790, 881322421), (1666326009, 407586031)));
            });
            assert!(post_fstree.undo_delta(&delta).is_err());
        }
    }

    #[test]
    fn test_undo_edit_wrong_object() {
        // undoing edit file but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1665521814, 361362753), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665557448, 433184586), "content 1")),
                Some(FSNode::file((1665591861, 496860274), "content 2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit file but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665714456, 921517643), |t| {
                t.add_file("some-file", (1665629918, 748815157), "content 0");
                t.add_symlink("some-symlink", (1665646889, 680207647), "path/to/0");
                t.add_empty_dir("some-dir", (1665671079, 418483515));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665749542, 493776715), "content 1")),
                Some(FSNode::file((1665773476, 126986370), "content 2")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit symlink but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665793485, 252381075), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665822311, 366487318), "path/to/1")),
                Some(FSNode::symlink((1665851905, 551315877), "path/to/2")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit symlink but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665979811, 781520250), |t| {
                t.add_file("some-file", (1665863625, 368006735), "content 0");
                t.add_symlink("some-symlink", (1665901150, 869201833), "path/to/0");
                t.add_empty_dir("some-dir", (1665932686, 971607668));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665987669, 495661603), "path/to/1")),
                Some(FSNode::symlink((1665996067, 478657676), "path/to/2")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit dir but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1666042297, 968081641), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_branch(
                "object",
                ((1666099964, 953749425), (1666099964, 953749425)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1666087020, 471242920), "content 1")),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1666099964, 953749425), "content 2")),
                    );
                },
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing edit dir but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1666209106, 3646732), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_branch(
                "object",
                ((1666259672, 116816453), (1666332978, 637258610)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1666230133, 997350604), "content 1")),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1666301106, 140598780), "content 2")),
                    );
                },
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_transmute_nonexisting() {
        // undoing transmute file to nonexisting symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object-0", (1664589837, 549365155), "path/to/0");
            t.add_symlink("object-1", (1664598913, 792002234), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::file((1664609336, 848178301), "content 3")),
                Some(FSNode::symlink((1664623468, 209967817), "path/to/2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to nonexisting dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object-0", (1664722087, 696952220), |t| {
                t.add_file("some-file", (1664633653, 739528540), "content 0");
                t.add_symlink("some-symlink", (1664678249, 437806961), "path/to/0");
                t.add_empty_dir("some-dir", (1664692981, 245962456));
            });
            t.add_dir("object-1", (1664831017, 841844411), |t| {
                t.add_file("some-file", (1664751265, 115732729), "content 1");
                t.add_symlink("some-symlink", (1664759232, 150924593), "path/to/1");
                t.add_empty_dir("some-dir", (1664806409, 246798122));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::file((1664876509, 934259071), "content 3")),
                Some(FSNode::dir((1664997199, 724350904), |t| {
                    t.add_file("some-file", (1664923659, 303459130), "content 2");
                    t.add_symlink("some-symlink", (1664950666, 770378360), "path/to/2");
                    t.add_empty_dir("some-dir", (1664965748, 458845045));
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute symlink to nonexisting file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object-0", (1665034391, 33703906), "content 0");
            t.add_file("object-1", (1665079350, 991536026), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::symlink((1665109890, 917779274), "path/to/3")),
                Some(FSNode::file((1665159178, 966621168), "content 2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute symlink to nonexisting dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object-0", (1665186908, 291192587), |t| {
                t.add_file("some-file", (1665192594, 649128708), "content 0");
                t.add_symlink("some-symlink", (1665230026, 336072556), "path/to/0");
                t.add_empty_dir("some-dir", (1665238886, 213959311));
            });
            t.add_dir("object-1", (1665273611, 834442083), |t| {
                t.add_file("some-file", (1665282388, 401924102), "content 1");
                t.add_symlink("some-symlink", (1665297354, 677155284), "path/to/1");
                t.add_empty_dir("some-dir", (1665303843, 467441652));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::symlink((1665331876, 631763394), "path/to/3")),
                Some(FSNode::dir((1665357606, 476090699), |t| {
                    t.add_file("some-file", (1665383234, 244877555), "content 2");
                    t.add_symlink("some-symlink", (1665406916, 813332093), "path/to/2");
                    t.add_empty_dir("some-dir", (1665455182, 477820574));
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to nonexisting file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object-0", (1665482084, 966733133), "content 0");
            t.add_file("object-1", (1665499356, 663792039), "content 1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::dir((1665510023, 465436417), |t| {
                    t.add_file("some-file", (1665520381, 361259197), "content 3");
                    t.add_symlink("some-symlink", (1665548312, 690402093), "path/to/3");
                    t.add_empty_dir("some-dir", (1665576110, 781672595));
                })),
                Some(FSNode::file((1665616314, 773647789), "content 2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to nonexisting symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object-0", (1665631092, 827582330), "path/to/0");
            t.add_symlink("object-1", (1665643000, 578539839), "path/to/1");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object-2",
                Some(FSNode::dir((1665656692, 400606844), |t| {
                    t.add_file("some-file", (1665675104, 864762471), "content 3");
                    t.add_symlink("some-symlink", (1665713581, 135631363), "path/to/3");
                    t.add_empty_dir("some-dir", (1665760984, 376321921));
                })),
                Some(FSNode::symlink((1665809727, 994141049), "path/to/2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_transmute_mismatching() {
        // undoing transmute symlink to mismatching file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1664624388, 173640720), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1664646854, 68417552), "path/to/2")),
                Some(FSNode::file((1664678881, 569613), "content 1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to mismatching file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1664699818, 511792465), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1664809563, 432676571), |t| {
                    t.add_file("some-file", (1664711226, 962906907), "content 2");
                    t.add_symlink("some-symlink", (1664739811, 284493436), "path/to/2");
                    t.add_empty_dir("some-dir", (1664764192, 641041531));
                })),
                Some(FSNode::file((1664838820, 196832056), "content 1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to mismatching symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664857064, 655845918), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1664884278, 973409107), "content 2")),
                Some(FSNode::symlink((1664921361, 659969228), "path/to/1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to mismatching symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664957970, 505634675), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665052904, 847790961), |t| {
                    t.add_file("some-file", (1664999570, 532898972), "content 2");
                    t.add_symlink("some-symlink", (1665010258, 147903028), "path/to/2");
                    t.add_empty_dir("some-dir", (1665031823, 24533559));
                })),
                Some(FSNode::symlink((1665088465, 649688454), "path/to/1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to mismatching dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665141950, 986516833), |t| {
                t.add_file("file", (1665105698, 834759767), "content 0");
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665162483, 371166997), "content 2")),
                Some(FSNode::dir((1665241016, 672403365), |t| {
                    t.add_file("file", (1665194000, 264453072), "content 1");
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute symlink to mismatching dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665254932, 540878635), |t| {
                t.add_file("file", (1665246274, 694486910), "content 0");
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665290222, 315496946), "path/to/2")),
                Some(FSNode::dir((1665350411, 211948902), |t| {
                    t.add_file("file", (1665325986, 886456024), "content 1");
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_undo_transmute_wrong_object() {
        // undoing transmute symlink to file but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664598830, 885688287), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1664626486, 869601720), "path/to/0")),
                Some(FSNode::file((1664663185, 725435206), "content 1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute symlink to file but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1664766982, 945261830), |t| {
                t.add_file("some-file", (1664689702, 561082585), "content 0");
                t.add_symlink("some-symlink", (1664714350, 207244979), "path/to/0");
                t.add_empty_dir("some-dir", (1664732919, 199494360));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1664803935, 936200875), "path/to/2")),
                Some(FSNode::file((1664845788, 542885548), "content 1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to file but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1664872397, 449056215), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665015274, 601376823), |t| {
                    t.add_file("some-file", (1664906663, 760045203), "content 2");
                    t.add_symlink("some-symlink", (1664943060, 766266988), "path/to/2");
                    t.add_empty_dir("some-dir", (1664981878, 487099947));
                })),
                Some(FSNode::file((1665052952, 502982116), "content 1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to file but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665168993, 400070001), |t| {
                t.add_file("some-file", (1665082479, 684032458), "content 0");
                t.add_symlink("some-symlink", (1665130979, 629936494), "path/to/0");
                t.add_empty_dir("some-dir", (1665144771, 393558792));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665324892, 690402221), |t| {
                    t.add_file("some-file", (1665207148, 548362627), "content 0");
                    t.add_symlink("some-symlink", (1665239436, 720846302), "path/to/0");
                    t.add_empty_dir("some-dir", (1665287427, 688533376));
                })),
                Some(FSNode::file((1665341261, 892507331), "content 1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to symlink but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665368849, 947458991), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665397871, 827676051), "content 0")),
                Some(FSNode::symlink((1665403142, 547311939), "path/to/1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to symlink but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665567238, 92003693), |t| {
                t.add_file("some-file", (1665446773, 512738294), "content 0");
                t.add_symlink("some-symlink", (1665490356, 271304382), "path/to/0");
                t.add_empty_dir("some-dir", (1665526657, 925635993));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665588889, 74877826), "content 3")),
                Some(FSNode::symlink((1665610964, 674205167), "path/to/2")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to symlink but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1665632969, 725474066), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665756640, 405575054), |t| {
                    t.add_file("some-file", (1665672534, 890373161), "content 2");
                    t.add_symlink("some-symlink", (1665710224, 343956707), "path/to/2");
                    t.add_empty_dir("some-dir", (1665728496, 190574305));
                })),
                Some(FSNode::symlink((1665783833, 825185769), "path/to/1")),
            )
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute dir to symlink but is dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("object", (1665893664, 624678638), |t| {
                t.add_file("some-file", (1665793423, 492677998), "content 0");
                t.add_symlink("some-symlink", (1665828085, 161279047), "path/to/0");
                t.add_empty_dir("some-dir", (1665868991, 374276931));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665981137, 650333053), |t| {
                    t.add_file("some-file", (1665921061, 523521189), "content 0");
                    t.add_symlink("some-symlink", (1665934508, 141263715), "path/to/0");
                    t.add_empty_dir("some-dir", (1665963185, 514111771));
                })),
                Some(FSNode::symlink((1666005781, 984020151), "path/to/1")),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to dir but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1666021142, 85179649), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1666028775, 13492626), "content 0")),
                Some(FSNode::dir((1666116279, 85720722), |t| {
                    t.add_file("some-file", (1666066763, 837798739), "content 1");
                    t.add_symlink("some-symlink", (1666077308, 122687056), "path/to/1");
                    t.add_empty_dir("some-dir", (1666088068, 163595375));
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to dir but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1666137170, 77684430), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1666269820, 579702059), "content 2")),
                Some(FSNode::dir((1666382185, 798514243), |t| {
                    t.add_file("some-file", (1666293069, 474711938), "content 1");
                    t.add_symlink("some-symlink", (1666339049, 342471756), "path/to/1");
                    t.add_empty_dir("some-dir", (1666368619, 605421377));
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute symlink to dir but is file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("object", (1666431611, 965475481), "content 0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1666469871, 209249656), "path/to/2")),
                Some(FSNode::dir((1666575485, 68302714), |t| {
                    t.add_file("some-file", (1666509074, 687282220), "content 1");
                    t.add_symlink("some-symlink", (1666518787, 497242816), "path/to/1");
                    t.add_empty_dir("some-dir", (1666546008, 540529529));
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // undoing transmute file to dir but is symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("object", (1666597164, 262663561), "path/to/0");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1666619381, 575827231), "path/to/0")),
                Some(FSNode::dir((1666729029, 103660010), |t| {
                    t.add_file("some-file", (1666628769, 409856969), "content 1");
                    t.add_symlink("some-symlink", (1666665157, 269582186), "path/to/1");
                    t.add_empty_dir("some-dir", (1666688206, 110203863));
                })),
            );
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }

    #[test]
    fn test_apply_undo_unshaken() {
        // unshaken delta on existing file
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1664592097, 696996980), "content");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf("file", None, None);
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // unshaken delta on existing symlink
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1664638764, 223835350), "path/to/somewhere");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf("symlink", None, None);
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // unshaken delta on existing dir
        let mut pre_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1664800569, 564665668), |t| {
                t.add_file("some-file", (1664688366, 82589160), "content");
                t.add_symlink("some-symlink", (1664730494, 873256012), "path/to/somewhere");
                t.add_empty_dir("some-dir", (1664767863, 610386210));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf("dir", None, None);
        });
        assert!(pre_fstree.apply_delta(&delta).is_err());

        // unshaken delta on (future) existing file
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_file("file", (1664834208, 754117368), "content");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf("file", None, None);
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // unshaken delta on (future) existing symlink
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1664844991, 256004858), "path/to/somewhere");
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf("symlink", None, None);
        });
        assert!(post_fstree.undo_delta(&delta).is_err());

        // unshaken delta on (future) existing dir
        let mut post_fstree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1664925614, 491308328), |t| {
                t.add_file("some-file", (1664852769, 95244362), "content");
                t.add_symlink("some-symlink", (1664871459, 80252076), "path/to/somewhere");
                t.add_empty_dir("some-dir", (1664887303, 584466861));
            });
        });
        let delta = Delta::gen_from(|d| {
            d.add_leaf("dir", None, None);
        });
        assert!(post_fstree.undo_delta(&delta).is_err());
    }
}
