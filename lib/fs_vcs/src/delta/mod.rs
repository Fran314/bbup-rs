use super::{hash_tree, ExcludeList, FSNode, FSTree};
use abst_fs::{AbstPath, Mtime};
use ior::{union, IOr};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod apply;
mod filter;
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{get_delta, hash_tree, Delta, DeltaNode, FSNode, FSTree};
    use abst_fs::{Endpoint, Mtime};
    use hasher::hash_bytes;

    #[test]
    fn test() {
        delta_node_impl();

        delta_empty();
        delta_shake();
        get();
    }

    fn delta_node_impl() {
        let mock_dir_node = FSNode::Dir(
            Mtime::from(1116035390, 33410985),
            hash_tree(&FSTree::empty()),
            FSTree::empty(),
        );
        let mock_file_node = FSNode::File(Mtime::from(667486944, 259193403), hash_bytes(b""));
        let mock_symlink_node = FSNode::SymLink(
            Mtime::from(1007746624, 413934774),
            hash_bytes(Endpoint::Unix(String::from("some/original/interesting/path")).as_bytes()),
        );

        assert_eq!(
            DeltaNode::remove(&mock_dir_node),
            DeltaNode::Leaf(Some(mock_dir_node.clone()), None)
        );
        assert_eq!(
            DeltaNode::remove(&mock_file_node),
            DeltaNode::Leaf(Some(mock_file_node.clone()), None)
        );
        assert_eq!(
            DeltaNode::remove(&mock_symlink_node),
            DeltaNode::Leaf(Some(mock_symlink_node.clone()), None)
        );

        assert_eq!(
            DeltaNode::add(&mock_dir_node),
            DeltaNode::Leaf(None, Some(mock_dir_node.clone()))
        );
        assert_eq!(
            DeltaNode::add(&mock_file_node),
            DeltaNode::Leaf(None, Some(mock_file_node.clone()))
        );
        assert_eq!(
            DeltaNode::add(&mock_symlink_node),
            DeltaNode::Leaf(None, Some(mock_symlink_node.clone()))
        );

        assert_eq!(
            DeltaNode::edit(&mock_dir_node, &mock_dir_node),
            DeltaNode::Leaf(Some(mock_dir_node.clone()), Some(mock_dir_node.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&mock_file_node, &mock_file_node),
            DeltaNode::Leaf(Some(mock_file_node.clone()), Some(mock_file_node.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&mock_symlink_node, &mock_symlink_node),
            DeltaNode::Leaf(
                Some(mock_symlink_node.clone()),
                Some(mock_symlink_node.clone())
            )
        );

        assert_eq!(
            DeltaNode::edit(&mock_dir_node, &mock_file_node),
            DeltaNode::Leaf(Some(mock_dir_node.clone()), Some(mock_file_node.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&mock_file_node, &mock_dir_node),
            DeltaNode::Leaf(Some(mock_file_node.clone()), Some(mock_dir_node.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&mock_file_node, &mock_symlink_node),
            DeltaNode::Leaf(
                Some(mock_file_node.clone()),
                Some(mock_symlink_node.clone())
            )
        );
        assert_eq!(
            DeltaNode::edit(&mock_symlink_node, &mock_file_node),
            DeltaNode::Leaf(
                Some(mock_symlink_node.clone()),
                Some(mock_file_node.clone())
            )
        );
        assert_eq!(
            DeltaNode::edit(&mock_symlink_node, &mock_dir_node),
            DeltaNode::Leaf(Some(mock_symlink_node.clone()), Some(mock_dir_node.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&mock_dir_node, &mock_symlink_node),
            DeltaNode::Leaf(Some(mock_dir_node.clone()), Some(mock_symlink_node.clone()))
        );
    }

    fn delta_empty() {
        assert_eq!(Delta::empty(), Delta(HashMap::from([])));
        assert!(Delta::empty().is_empty());

        let mock_file_node = FSNode::File(Mtime::from(1120127351, 306920486), hash_bytes(b""));
        let mock_delta_node = DeltaNode::add(&mock_file_node);
        let non_empty_delta = Delta(HashMap::from([(String::from("file"), mock_delta_node)]));
        assert!(!non_empty_delta.is_empty());
    }

    fn delta_shake() {
        let mut unshaken_delta = {
            let branch1 = DeltaNode::Branch(None, Delta::empty());
            let branch2 = {
                let mock_dir_node = FSNode::Dir(
                    Mtime::from(1063113618, 113746745),
                    hash_tree(&FSTree::empty()),
                    FSTree::empty(),
                );
                let mock_file_node =
                    FSNode::File(Mtime::from(593849028, 842039177), hash_bytes(b""));
                let mock_symlink_node = FSNode::SymLink(
                    Mtime::from(984260842, 571979684),
                    hash_bytes(
                        Endpoint::Unix(String::from("some/original/interesting/path")).as_bytes(),
                    ),
                );

                let leaf1 = DeltaNode::Leaf(None, None);
                let leaf2 =
                    DeltaNode::Leaf(Some(mock_dir_node.clone()), Some(mock_dir_node.clone()));
                let leaf3 =
                    DeltaNode::Leaf(Some(mock_file_node.clone()), Some(mock_file_node.clone()));
                let leaf4 = DeltaNode::Leaf(
                    Some(mock_symlink_node.clone()),
                    Some(mock_symlink_node.clone()),
                );

                let branch = DeltaNode::Branch(None, Delta::empty());

                DeltaNode::Branch(
                    None,
                    Delta(HashMap::from([
                        (String::from("leaf1"), leaf1),
                        (String::from("leaf2"), leaf2),
                        (String::from("leaf3"), leaf3),
                        (String::from("leaf4"), leaf4),
                        (String::from("branch"), branch),
                    ])),
                )
            };

            let leaf = {
                let pre_dir = {
                    let mock_file_node = FSNode::File(
                        Mtime::from(777784611, 365943901),
                        hash_bytes(b"some file with content"),
                    );
                    let subtree =
                        FSTree(HashMap::from([(String::from("file.txt"), mock_file_node)]));
                    FSNode::Dir(
                        Mtime::from(1283460680, 617587361),
                        hash_tree(&subtree),
                        subtree,
                    )
                };
                let post_dir = {
                    let mock_file_node = FSNode::File(
                        Mtime::from(1283460680, 617587361),
                        hash_bytes(b"some other file with different content"),
                    );
                    let subtree = FSTree(HashMap::from([(
                        String::from("other-file.txt"),
                        mock_file_node,
                    )]));
                    FSNode::Dir(
                        Mtime::from(539622622, 899584595),
                        hash_tree(&subtree),
                        subtree,
                    )
                };
                DeltaNode::Leaf(Some(pre_dir), Some(post_dir))
            };

            Delta(HashMap::from([
                (String::from("branch1"), branch1),
                (String::from("branch2"), branch2),
                (String::from("leaf"), leaf),
            ]))
        };

        let shaken_delta = {
            let shaken_leaf = {
                let removed_file = FSNode::File(
                    Mtime::from(777784611, 365943901),
                    hash_bytes(b"some file with content"),
                );
                let added_other_file = FSNode::File(
                    Mtime::from(1283460680, 617587361),
                    hash_bytes(b"some other file with different content"),
                );
                let delta = Delta(HashMap::from([
                    (
                        String::from("file.txt"),
                        DeltaNode::Leaf(Some(removed_file), None),
                    ),
                    (
                        String::from("other-file.txt"),
                        DeltaNode::Leaf(None, Some(added_other_file)),
                    ),
                ]));
                DeltaNode::Branch(
                    Some((
                        Mtime::from(1283460680, 617587361),
                        Mtime::from(539622622, 899584595),
                    )),
                    delta,
                )
            };
            Delta(HashMap::from([(String::from("leaf"), shaken_leaf)]))
        };

        unshaken_delta.shake();
        assert_eq!(unshaken_delta, shaken_delta);
    }

    fn get() {
        let contentful_dir: FSNode = {
            let inner_dir = {
                let inner_file_2 = FSNode::File(
                    Mtime::from(1598728573, 546351705),
                    hash_bytes(b"hey there's content!"),
                );
                let inner_symlink_2 = FSNode::SymLink(
                    Mtime::from(1598728573, 546351705),
                    hash_bytes(Endpoint::Unix(String::from("where/is/this/path/going")).as_bytes()),
                );
                let inner_dir_2 = FSNode::Dir(
                    Mtime::from(590816735, 667223352),
                    hash_tree(&FSTree::empty()),
                    FSTree::empty(),
                );

                let subtree = FSTree(HashMap::from([
                    (String::from("inner-file-2"), inner_file_2),
                    (String::from("inner-symlink-2"), inner_symlink_2),
                    (String::from("inner-dir-2"), inner_dir_2),
                ]));
                FSNode::Dir(
                    Mtime::from(1364181678, 477789959),
                    hash_tree(&subtree),
                    subtree,
                )
            };
            let inner_file = FSNode::File(
                Mtime::from(997012509, 922451121),
                hash_bytes(b"this is some cool content"),
            );
            let inner_symlink = FSNode::SymLink(
                Mtime::from(808926076, 398339329),
                hash_bytes(Endpoint::Unix(String::from("path/that/goes/nowhere")).as_bytes()),
            );

            let subtree = FSTree(HashMap::from([
                (String::from("inner-file"), inner_file),
                (String::from("inner-symlink"), inner_symlink),
                (String::from("inner-dir"), inner_dir),
            ]));
            FSNode::Dir(
                Mtime::from(1364181678, 477789959),
                hash_tree(&subtree),
                subtree,
            )
        };

        let pre_fstree = {
            let mtime_edit_dir = FSNode::Dir(
                Mtime::from(667322229, 283834161),
                hash_tree(&FSTree::empty()),
                FSTree::empty(),
            );
            let content_edit_dir = {
                let both_edit_dir = {
                    let removed_file = FSNode::File(
                        Mtime::from(1213260096, 785625266),
                        hash_bytes(b"some content"),
                    );
                    let removed_symlink = FSNode::SymLink(
                        Mtime::from(808926076, 398339329),
                        hash_bytes(
                            Endpoint::Unix(String::from("you/won't/see/this/path/later"))
                                .as_bytes(),
                        ),
                    );
                    let removed_dir = contentful_dir.clone();

                    let mtime_edit_file = FSNode::File(
                        Mtime::from(1611850953, 971525938),
                        hash_bytes(b"the content of this file won't change"),
                    );
                    let content_edit_file = FSNode::File(
                        Mtime::from(1245890614, 586345017),
                        hash_bytes(b"the content of this file will change, but the mtime won't"),
                    );
                    let both_edit_file = FSNode::File(
                        Mtime::from(1245890614, 586345017),
                        hash_bytes(b"both the content and the mtime of the file will change"),
                    );

                    let mtime_edit_symlink = FSNode::SymLink(
                        Mtime::from(1245890614, 586345017),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/path/will/not/change")).as_bytes(),
                        ),
                    );
                    let endpoint_edit_symlink = FSNode::SymLink(
                        Mtime::from(1397507428, 322887183),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/path/will/change")).as_bytes(),
                        ),
                    );
                    let both_edit_symlink = FSNode::SymLink(
                        Mtime::from(547336108, 262002124),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/path/will/change/and/the/mtime/too"))
                                .as_bytes(),
                        ),
                    );

                    let file_to_dir = FSNode::File(
                        Mtime::from(547336108, 262002124),
                        hash_bytes(b"the file will become a directory"),
                    );
                    let file_to_symlink = FSNode::File(
                        Mtime::from(1396993467, 652868396),
                        hash_bytes(b"the file will become a symlink"),
                    );
                    let symlink_to_file = FSNode::SymLink(
                        Mtime::from(776329648, 625499475),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/symlink/will/become/a/file"))
                                .as_bytes(),
                        ),
                    );
                    let symlink_to_dir = FSNode::SymLink(
                        Mtime::from(1493041434, 347743433),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/symlink/will/become/a/directory"))
                                .as_bytes(),
                        ),
                    );
                    let dir_to_file = contentful_dir.clone();
                    let dir_to_symlink = contentful_dir.clone();

                    let subtree = FSTree(HashMap::from([
                        (String::from("removed-file"), removed_file),
                        (String::from("removed-symlink"), removed_symlink),
                        (String::from("removed-dir"), removed_dir),
                        (String::from("mtime-edit-file"), mtime_edit_file),
                        (String::from("content-edit-file"), content_edit_file),
                        (String::from("both-edit-file"), both_edit_file),
                        (String::from("mtime-edit-symlink"), mtime_edit_symlink),
                        (String::from("endpoint-edit-symlink"), endpoint_edit_symlink),
                        (String::from("both-edit-symlink"), both_edit_symlink),
                        (String::from("file-to-dir"), file_to_dir),
                        (String::from("file-to-symlink"), file_to_symlink),
                        (String::from("symlink-to-dir"), symlink_to_dir),
                        (String::from("symlink-to-file"), symlink_to_file),
                        (String::from("dir-to-file"), dir_to_file),
                        (String::from("dir-to-symlink"), dir_to_symlink),
                    ]));
                    FSNode::Dir(
                        Mtime::from(820956170, 426474588),
                        hash_tree(&subtree),
                        subtree,
                    )
                };

                let subtree = FSTree(HashMap::from([(
                    String::from("both-edit-dir"),
                    both_edit_dir,
                )]));
                FSNode::Dir(
                    Mtime::from(995819275, 864246209),
                    hash_tree(&subtree),
                    subtree,
                )
            };
            FSTree(HashMap::from([
                (String::from("mtime-edit-dir"), mtime_edit_dir),
                (String::from("content-edit-dir"), content_edit_dir),
            ]))
        };
        let post_fstree = {
            let mtime_edit_dir = FSNode::Dir(
                Mtime::from(1428359331, 168489967),
                hash_tree(&FSTree::empty()),
                FSTree::empty(),
            );
            let content_edit_dir = {
                let both_edit_dir = {
                    let added_file = FSNode::File(
                        Mtime::from(1029314114, 210225767),
                        hash_bytes(b"this file has been added"),
                    );
                    let added_symlink = FSNode::SymLink(
                        Mtime::from(999001645, 810306108),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/path/didn't/exist/before"))
                                .as_bytes(),
                        ),
                    );
                    let added_dir = contentful_dir.clone();

                    let mtime_edit_file = FSNode::File(
                        Mtime::from(1048587011, 445332193),
                        hash_bytes(b"the content of this file won't change"),
                    );
                    let content_edit_file = FSNode::File(
                        Mtime::from(1245890614, 586345017),
                        hash_bytes(b"the content of this file has changed, but the mtime didn't"),
                    );
                    let both_edit_file = FSNode::File(
                        Mtime::from(815892169, 640255056),
                        hash_bytes(b"both the content and the mtime of the file have changed"),
                    );

                    let mtime_edit_symlink = FSNode::SymLink(
                        Mtime::from(692432309, 274032817),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/path/will/not/change")).as_bytes(),
                        ),
                    );
                    let endpoint_edit_symlink = FSNode::SymLink(
                        Mtime::from(1397507428, 322887183),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/path/has/changed")).as_bytes(),
                        ),
                    );
                    let both_edit_symlink = FSNode::SymLink(
                        Mtime::from(1274175004, 906839206),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/path/has/changed/and/the/mtime/too"))
                                .as_bytes(),
                        ),
                    );

                    let file_to_dir = contentful_dir.clone();
                    let file_to_symlink = FSNode::SymLink(
                        Mtime::from(616276621, 81572476),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/symlink/used/to/be/a/file"))
                                .as_bytes(),
                        ),
                    );
                    let symlink_to_file = FSNode::File(
                        Mtime::from(1538377515, 691983830),
                        hash_bytes(b"this file used to be a symlink!"),
                    );
                    let symlink_to_dir = contentful_dir.clone();
                    let dir_to_file = FSNode::File(
                        Mtime::from(1602277549, 958804909),
                        hash_bytes(b"this file used to be a directory!"),
                    );
                    let dir_to_symlink = FSNode::SymLink(
                        Mtime::from(1122800412, 992618853),
                        hash_bytes(
                            Endpoint::Unix(String::from("this/symlink/used/to/be/a/directory"))
                                .as_bytes(),
                        ),
                    );

                    let subtree = FSTree(HashMap::from([
                        (String::from("added-file"), added_file),
                        (String::from("added-symlink"), added_symlink),
                        (String::from("added-dir"), added_dir),
                        (String::from("mtime-edit-file"), mtime_edit_file),
                        (String::from("content-edit-file"), content_edit_file),
                        (String::from("both-edit-file"), both_edit_file),
                        (String::from("mtime-edit-symlink"), mtime_edit_symlink),
                        (String::from("endpoint-edit-symlink"), endpoint_edit_symlink),
                        (String::from("both-edit-symlink"), both_edit_symlink),
                        (String::from("file-to-dir"), file_to_dir),
                        (String::from("file-to-symlink"), file_to_symlink),
                        (String::from("symlink-to-file"), symlink_to_file),
                        (String::from("symlink-to-dir"), symlink_to_dir),
                        (String::from("dir-to-file"), dir_to_file),
                        (String::from("dir-to-symlink"), dir_to_symlink),
                    ]));
                    FSNode::Dir(
                        Mtime::from(1535927666, 535018497),
                        hash_tree(&subtree),
                        subtree,
                    )
                };

                let subtree = FSTree(HashMap::from([(
                    String::from("both-edit-dir"),
                    both_edit_dir,
                )]));
                FSNode::Dir(
                    Mtime::from(995819275, 864246209),
                    hash_tree(&subtree),
                    subtree,
                )
            };
            FSTree(HashMap::from([
                (String::from("mtime-edit-dir"), mtime_edit_dir),
                (String::from("content-edit-dir"), content_edit_dir),
            ]))
        };

        let supposed_delta: Delta = {
            let mtime_edit_dir = DeltaNode::Branch(
                Some((
                    Mtime::from(667322229, 283834161),
                    Mtime::from(1428359331, 168489967),
                )),
                Delta::empty(),
            );
            let content_edit_dir: DeltaNode = {
                let both_edit_dir: DeltaNode = {
                    let removed_file = {
                        let pre = FSNode::File(
                            Mtime::from(1213260096, 785625266),
                            hash_bytes(b"some content"),
                        );
                        DeltaNode::Leaf(Some(pre), None)
                    };
                    let removed_symlink = {
                        let pre = FSNode::SymLink(
                            Mtime::from(808926076, 398339329),
                            hash_bytes(
                                Endpoint::Unix(String::from("you/won't/see/this/path/later"))
                                    .as_bytes(),
                            ),
                        );
                        DeltaNode::Leaf(Some(pre), None)
                    };
                    let removed_dir = {
                        let pre = contentful_dir.clone();
                        DeltaNode::Leaf(Some(pre), None)
                    };

                    let added_file = {
                        let post = FSNode::File(
                            Mtime::from(1029314114, 210225767),
                            hash_bytes(b"this file has been added"),
                        );
                        DeltaNode::Leaf(None, Some(post))
                    };
                    let added_symlink = {
                        let post = FSNode::SymLink(
                            Mtime::from(999001645, 810306108),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/path/didn't/exist/before"))
                                    .as_bytes(),
                            ),
                        );
                        DeltaNode::Leaf(None, Some(post))
                    };
                    let added_dir = {
                        let post = contentful_dir.clone();
                        DeltaNode::Leaf(None, Some(post))
                    };

                    let mtime_edit_file = {
                        let pre = FSNode::File(
                            Mtime::from(1611850953, 971525938),
                            hash_bytes(b"the content of this file won't change"),
                        );
                        let post = FSNode::File(
                            Mtime::from(1048587011, 445332193),
                            hash_bytes(b"the content of this file won't change"),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let content_edit_file = {
                        let pre = FSNode::File(
                            Mtime::from(1245890614, 586345017),
                            hash_bytes(
                                b"the content of this file will change, but the mtime won't",
                            ),
                        );
                        let post = FSNode::File(
                            Mtime::from(1245890614, 586345017),
                            hash_bytes(
                                b"the content of this file has changed, but the mtime didn't",
                            ),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let both_edit_file = {
                        let pre = FSNode::File(
                            Mtime::from(1245890614, 586345017),
                            hash_bytes(b"both the content and the mtime of the file will change"),
                        );
                        let post = FSNode::File(
                            Mtime::from(815892169, 640255056),
                            hash_bytes(b"both the content and the mtime of the file have changed"),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };

                    let mtime_edit_symlink = {
                        let pre = FSNode::SymLink(
                            Mtime::from(1245890614, 586345017),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/path/will/not/change"))
                                    .as_bytes(),
                            ),
                        );
                        let post = FSNode::SymLink(
                            Mtime::from(692432309, 274032817),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/path/will/not/change"))
                                    .as_bytes(),
                            ),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let endpoint_edit_symlink = {
                        let pre = FSNode::SymLink(
                            Mtime::from(1397507428, 322887183),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/path/will/change")).as_bytes(),
                            ),
                        );
                        let post = FSNode::SymLink(
                            Mtime::from(1397507428, 322887183),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/path/has/changed")).as_bytes(),
                            ),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let both_edit_symlink = {
                        let pre = FSNode::SymLink(
                            Mtime::from(547336108, 262002124),
                            hash_bytes(
                                Endpoint::Unix(String::from(
                                    "this/path/will/change/and/the/mtime/too",
                                ))
                                .as_bytes(),
                            ),
                        );
                        let post = FSNode::SymLink(
                            Mtime::from(1274175004, 906839206),
                            hash_bytes(
                                Endpoint::Unix(String::from(
                                    "this/path/has/changed/and/the/mtime/too",
                                ))
                                .as_bytes(),
                            ),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };

                    let file_to_dir = {
                        let pre = FSNode::File(
                            Mtime::from(547336108, 262002124),
                            hash_bytes(b"the file will become a directory"),
                        );
                        let post = contentful_dir.clone();
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let file_to_symlink = {
                        let pre = FSNode::File(
                            Mtime::from(1396993467, 652868396),
                            hash_bytes(b"the file will become a symlink"),
                        );
                        let post = FSNode::SymLink(
                            Mtime::from(616276621, 81572476),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/symlink/used/to/be/a/file"))
                                    .as_bytes(),
                            ),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let symlink_to_file = {
                        let pre = FSNode::SymLink(
                            Mtime::from(776329648, 625499475),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/symlink/will/become/a/file"))
                                    .as_bytes(),
                            ),
                        );
                        let post = FSNode::File(
                            Mtime::from(1538377515, 691983830),
                            hash_bytes(b"this file used to be a symlink!"),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let symlink_to_dir = {
                        let pre = FSNode::SymLink(
                            Mtime::from(1493041434, 347743433),
                            hash_bytes(
                                Endpoint::Unix(String::from(
                                    "this/symlink/will/become/a/directory",
                                ))
                                .as_bytes(),
                            ),
                        );
                        let post = contentful_dir.clone();
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let dir_to_file = {
                        let pre = contentful_dir.clone();
                        let post = FSNode::File(
                            Mtime::from(1602277549, 958804909),
                            hash_bytes(b"this file used to be a directory!"),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };
                    let dir_to_symlink = {
                        let pre = contentful_dir.clone();
                        let post = FSNode::SymLink(
                            Mtime::from(1122800412, 992618853),
                            hash_bytes(
                                Endpoint::Unix(String::from("this/symlink/used/to/be/a/directory"))
                                    .as_bytes(),
                            ),
                        );
                        DeltaNode::Leaf(Some(pre), Some(post))
                    };

                    let subdelta = Delta(HashMap::from([
                        (String::from("removed-file"), removed_file),
                        (String::from("removed-symlink"), removed_symlink),
                        (String::from("removed-dir"), removed_dir),
                        (String::from("added-file"), added_file),
                        (String::from("added-symlink"), added_symlink),
                        (String::from("added-dir"), added_dir),
                        (String::from("mtime-edit-file"), mtime_edit_file),
                        (String::from("content-edit-file"), content_edit_file),
                        (String::from("both-edit-file"), both_edit_file),
                        (String::from("mtime-edit-symlink"), mtime_edit_symlink),
                        (String::from("endpoint-edit-symlink"), endpoint_edit_symlink),
                        (String::from("both-edit-symlink"), both_edit_symlink),
                        (String::from("file-to-dir"), file_to_dir),
                        (String::from("file-to-symlink"), file_to_symlink),
                        (String::from("symlink-to-file"), symlink_to_file),
                        (String::from("symlink-to-dir"), symlink_to_dir),
                        (String::from("dir-to-file"), dir_to_file),
                        (String::from("dir-to-symlink"), dir_to_symlink),
                    ]));
                    DeltaNode::Branch(
                        Some((
                            Mtime::from(820956170, 426474588),
                            Mtime::from(1535927666, 535018497),
                        )),
                        subdelta,
                    )
                };

                let subdelta = Delta(HashMap::from([(
                    String::from("both-edit-dir"),
                    both_edit_dir,
                )]));
                DeltaNode::Branch(None, subdelta)
            };
            Delta(HashMap::from([
                (String::from("mtime-edit-dir"), mtime_edit_dir),
                (String::from("content-edit-dir"), content_edit_dir),
            ]))
        };

        assert_eq!(supposed_delta, get_delta(&pre_fstree, &post_fstree));

        let mut fstree_to_upgrade = pre_fstree.clone();
        fstree_to_upgrade.apply_delta(&supposed_delta).unwrap();
        assert_eq!(fstree_to_upgrade, post_fstree);

        let mut fstree_to_downgrade = post_fstree.clone();
        fstree_to_downgrade.undo_delta(&supposed_delta).unwrap();
        assert_eq!(fstree_to_downgrade, pre_fstree);
    }
}
