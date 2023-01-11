use super::{hash_tree, ExcludeList, FSNode, FSTree};
use abst_fs::{AbstPath, Mtime};
use ior::{union, IOr};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod apply;
mod filter;
mod merge;
mod rebase;

pub use merge::UnmergeableDelta;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DeltaNode {
    Leaf(Option<FSNode>, Option<FSNode>),
    Branch((Mtime, Mtime), Delta),
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
                    // We assume here that if a delta is generated with
                    // get_delta then it is automatically already shaken, and no
                    // recursion is needed
                    *entry = Branch((m0.clone(), m1.clone()), get_delta(subtree0, subtree1));
                }
                Branch(_, subdelta) => subdelta.shake(),
                Leaf(_, _) => {}
            }
        }
        tree.retain(|_, child| match child {
            Leaf(pre, post) => pre != post,
            Branch((premtime, postmtime), subdelta) => {
                premtime != postmtime || (!subdelta.is_empty())
            }
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
                        let delta_subtree = match h0.ne(h1) {
                            true => get_delta(subtree0, subtree1),
                            false => Delta::empty(),
                        };
                        delta.insert(
                            key,
                            DeltaNode::Branch((m0.clone(), m1.clone()), delta_subtree),
                        );
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

    use super::{get_delta, Delta, DeltaNode, FSNode, FSTree};

    use abst_fs::Mtime;

    impl DeltaNode {
        pub fn leaf(pre: Option<FSNode>, post: Option<FSNode>) -> DeltaNode {
            DeltaNode::Leaf(pre, post)
        }
        pub fn branch(
            ((presec, prenano), (postsec, postnano)): ((i64, u32), (i64, u32)),
            subdelta_gen: impl Fn(&mut Delta),
        ) -> DeltaNode {
            let mtimes = (Mtime::from(presec, prenano), Mtime::from(postsec, postnano));
            let mut subdelta = Delta::empty();
            subdelta_gen(&mut subdelta);
            DeltaNode::Branch(mtimes, subdelta)
        }
        pub fn empty_branch(
            ((presec, prenano), (postsec, postnano)): ((i64, u32), (i64, u32)),
        ) -> DeltaNode {
            let mtimes = (Mtime::from(presec, prenano), Mtime::from(postsec, postnano));
            DeltaNode::Branch(mtimes, Delta::empty())
        }
    }

    impl Delta {
        pub fn gen_from(gen: impl Fn(&mut Delta)) -> Delta {
            let mut delta = Delta::empty();
            gen(&mut delta);
            delta
        }
        pub fn add_leaf(&mut self, name: impl ToString, pre: Option<FSNode>, post: Option<FSNode>) {
            let Delta(map) = self;
            map.insert(name.to_string(), DeltaNode::leaf(pre, post));
        }
        pub fn add_branch(
            &mut self,
            name: impl ToString,
            mtimes: ((i64, u32), (i64, u32)),
            subdelta_gen: impl Fn(&mut Delta),
        ) {
            let Delta(map) = self;
            map.insert(name.to_string(), DeltaNode::branch(mtimes, subdelta_gen));
        }
        pub fn add_empty_branch(&mut self, name: impl ToString, mtimes: ((i64, u32), (i64, u32))) {
            let Delta(map) = self;
            map.insert(name.to_string(), DeltaNode::empty_branch(mtimes));
        }
    }

    #[test]
    fn test() {
        delta_node_impl();

        delta_empty();
        delta_shake();
        get();
    }

    fn delta_node_impl() {
        let dir = FSNode::empty_dir((1116035390, 33410985));
        let file = FSNode::file((667486944, 259193403), "");
        let symlink = FSNode::symlink((1007746624, 413934774), "some/original/interesting/path");

        assert_eq!(
            DeltaNode::remove(&dir),
            DeltaNode::leaf(Some(dir.clone()), None)
        );
        assert_eq!(
            DeltaNode::remove(&file),
            DeltaNode::leaf(Some(file.clone()), None)
        );
        assert_eq!(
            DeltaNode::remove(&symlink),
            DeltaNode::leaf(Some(symlink.clone()), None)
        );

        assert_eq!(
            DeltaNode::add(&dir),
            DeltaNode::leaf(None, Some(dir.clone()))
        );
        assert_eq!(
            DeltaNode::add(&file),
            DeltaNode::leaf(None, Some(file.clone()))
        );
        assert_eq!(
            DeltaNode::add(&symlink),
            DeltaNode::leaf(None, Some(symlink.clone()))
        );

        assert_eq!(
            DeltaNode::edit(&dir, &dir),
            DeltaNode::leaf(Some(dir.clone()), Some(dir.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&file, &file),
            DeltaNode::leaf(Some(file.clone()), Some(file.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&symlink, &symlink),
            DeltaNode::leaf(Some(symlink.clone()), Some(symlink.clone()))
        );

        assert_eq!(
            DeltaNode::edit(&dir, &file),
            DeltaNode::leaf(Some(dir.clone()), Some(file.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&file, &dir),
            DeltaNode::leaf(Some(file.clone()), Some(dir.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&file, &symlink),
            DeltaNode::leaf(Some(file.clone()), Some(symlink.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&symlink, &file),
            DeltaNode::leaf(Some(symlink.clone()), Some(file.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&symlink, &dir),
            DeltaNode::leaf(Some(symlink.clone()), Some(dir.clone()))
        );
        assert_eq!(
            DeltaNode::edit(&dir, &symlink),
            DeltaNode::leaf(Some(dir.clone()), Some(symlink.clone()))
        );
    }

    fn delta_empty() {
        assert_eq!(Delta::empty(), Delta(HashMap::from([])));
        assert!(Delta::empty().is_empty());

        let mock_file_node = FSNode::file((1120127351, 306920486), "");
        let mock_delta_node = DeltaNode::add(&mock_file_node);
        let non_empty_delta = Delta(HashMap::from([(String::from("file"), mock_delta_node)]));
        assert!(!non_empty_delta.is_empty());
    }

    fn delta_shake() {
        // TODO rewrite this test in standardized name and content form
        let mut unshaken_delta = Delta::gen_from(|d| {
            d.add_empty_branch(
                "branch1",
                ((1664579039, 891307232), (1664579039, 891307232)),
            );
            d.add_branch(
                "branch2",
                ((1664595484, 947041845), (1664595484, 947041845)),
                |d| {
                    let dir = FSNode::empty_dir((1664614263, 371254296));
                    let file = FSNode::file((1664644221, 832021560), "");
                    let symlink =
                        FSNode::symlink((1664808823, 723934215), "some/original/interesting/path");

                    d.add_leaf("leaf1", None, None);
                    d.add_leaf("leaf2", Some(dir.clone()), Some(dir));
                    d.add_leaf("leaf3", Some(file.clone()), Some(file));
                    d.add_leaf("leaf4", Some(symlink.clone()), Some(symlink));
                    d.add_empty_branch(
                        "branch",
                        ((1664866042, 747401141), (1664866042, 747401141)),
                    );
                },
            );
            d.add_leaf(
                "leaf",
                Some(FSNode::dir((1664876366, 977774592), |t| {
                    t.add_file("file.txt", (1664905600, 230037082), "file with content");
                })),
                Some(FSNode::dir((1664935601, 700740412), |t| {
                    t.add_file("other-file.txt", (1664916387, 189221502), "different file");
                })),
            );
        });

        let shaken_delta = Delta::gen_from(|d| {
            d.add_branch(
                "leaf",
                ((1664876366, 977774592), (1664935601, 700740412)),
                |d| {
                    d.add_leaf(
                        "file.txt",
                        Some(FSNode::file((1664905600, 230037082), "file with content")),
                        None,
                    );
                    d.add_leaf(
                        "other-file.txt",
                        None,
                        Some(FSNode::file((1664916387, 189221502), "different file")),
                    );
                },
            );
        });

        unshaken_delta.shake();
        assert_eq!(unshaken_delta, shaken_delta);
    }

    fn get() {
        // TODO rewrite this test in standardized name and content form
        let mock_dir_content = |t: &mut FSTree| {
            t.add_file("file", (997012509, 922451121), "mock content");
            t.add_symlink("symlink", (808926076, 398339329), "mock/path/to/nowhere");
            t.add_dir("dir", (1364181678, 477789959), |t| {
                t.add_file("file2", (1598728573, 546351705), "mock content 2");
                t.add_symlink("symlink2", (1598728573, 546351705), "mock/path/2");
                t.add_empty_dir("dir2", (590816735, 667223352));
            })
        };

        let pre_fstree = FSTree::gen_from(|t| {
            t.add_empty_dir("mtime-edit-dir", (667322229, 283834161));
            t.add_dir("content-edit-dir", (995819275, 864246209), |t| {
                t.add_dir("both-edit-dir", (820956170, 426474588), |t| {
                    t.add_file("removed-file", (1213260096, 785625266), "some content");
                    t.add_symlink("removed-symlink", (808926076, 398339329), "some/fake/path");
                    t.add_dir("removed-dir", (1364181678, 477789959), mock_dir_content);

                    t.add_file("mtime-edit-file", (1611850953, 971525938), "fixed content");
                    t.add_file(
                        "content-edit-file",
                        (1245890614, 586345017),
                        "changed content",
                    );
                    t.add_file(
                        "both-edit-file",
                        (1245890614, 586345017),
                        "changed content and mtime",
                    );

                    t.add_symlink("mtime-edit-symlink", (1245890614, 586345017), "fixed/path");
                    t.add_symlink(
                        "endpoint-edit-symlink",
                        (1397507428, 322887183),
                        "changed/path",
                    );
                    t.add_symlink(
                        "both-edit-symlink",
                        (547336108, 262002124),
                        "changed/path/and/mtime",
                    );

                    t.add_file("file-to-dir", (547336108, 262002124), "mock content");
                    t.add_file("file-to-symlink", (1396993467, 652868396), "fake content");

                    t.add_symlink("symlink-to-file", (776329648, 625499475), "random/path");
                    t.add_symlink("symlink-to-dir", (1493041434, 347743433), "fake/path");
                    t.add_dir("dir-to-file", (1364181678, 477789959), mock_dir_content);
                    t.add_dir("dir-to-symlink", (1364181678, 477789959), mock_dir_content);
                });
            });
        });

        let post_fstree = FSTree::gen_from(|t| {
            t.add_empty_dir("mtime-edit-dir", (1428359331, 168489967));
            t.add_dir("content-edit-dir", (995819275, 864246209), |t| {
                t.add_dir("both-edit-dir", (1535927666, 535018497), |t| {
                    t.add_file("added-file", (1029314114, 210225767), "added content");
                    t.add_symlink("added-symlink", (999001645, 810306108), "new/fake/path");
                    t.add_dir("added-dir", (1364181678, 477789959), mock_dir_content);

                    t.add_file("mtime-edit-file", (1048587011, 445332193), "fixed content");
                    t.add_file(
                        "content-edit-file",
                        (1245890614, 586345017),
                        "content has changed",
                    );
                    t.add_file(
                        "both-edit-file",
                        (815892169, 640255056),
                        "content and mtime have changed",
                    );

                    t.add_symlink("mtime-edit-symlink", (692432309, 274032817), "fixed/path");
                    t.add_symlink(
                        "endpoint-edit-symlink",
                        (1397507428, 322887183),
                        "path/did/change",
                    );
                    t.add_symlink(
                        "both-edit-symlink",
                        (1274175004, 906839206),
                        "path/and/mtime/did/change/",
                    );

                    t.add_dir("file-to-dir", (1364181678, 477789959), mock_dir_content);
                    t.add_symlink("file-to-symlink", (616276621, 81572476), "fake/new/path");

                    t.add_file("symlink-to-file", (1538377515, 691983830), "this is file");
                    t.add_dir("symlink-to-dir", (1364181678, 477789959), mock_dir_content);

                    t.add_file("dir-to-file", (1602277549, 958804909), "not dir anymore");
                    t.add_symlink("dir-to-symlink", (1122800412, 992618853), "also/not/dir");
                });
            });
        });

        let supposed_delta = Delta::gen_from(|d| {
            d.add_empty_branch(
                "mtime-edit-dir",
                ((667322229, 283834161), (1428359331, 168489967)),
            );
            d.add_branch(
                "content-edit-dir",
                ((995819275, 864246209), (995819275, 864246209)),
                |d| {
                    d.add_branch(
                        "both-edit-dir",
                        ((820956170, 426474588), (1535927666, 535018497)),
                        |d| {
                            d.add_leaf(
                                "removed-file",
                                Some(FSNode::file((1213260096, 785625266), "some content")),
                                None,
                            );
                            d.add_leaf(
                                "removed-symlink",
                                Some(FSNode::symlink((808926076, 398339329), "some/fake/path")),
                                None,
                            );
                            d.add_leaf(
                                "removed-dir",
                                Some(FSNode::dir((1364181678, 477789959), mock_dir_content)),
                                None,
                            );

                            d.add_leaf(
                                "added-file",
                                None,
                                Some(FSNode::file((1029314114, 210225767), "added content")),
                            );
                            d.add_leaf(
                                "added-symlink",
                                None,
                                Some(FSNode::symlink((999001645, 810306108), "new/fake/path")),
                            );
                            d.add_leaf(
                                "added-dir",
                                None,
                                Some(FSNode::dir((1364181678, 477789959), mock_dir_content)),
                            );

                            d.add_leaf(
                                "mtime-edit-file",
                                Some(FSNode::file((1611850953, 971525938), "fixed content")),
                                Some(FSNode::file((1048587011, 445332193), "fixed content")),
                            );
                            d.add_leaf(
                                "content-edit-file",
                                Some(FSNode::file((1245890614, 586345017), "changed content")),
                                Some(FSNode::file((1245890614, 586345017), "content has changed")),
                            );
                            d.add_leaf(
                                "both-edit-file",
                                Some(FSNode::file(
                                    (1245890614, 586345017),
                                    "changed content and mtime",
                                )),
                                Some(FSNode::file(
                                    (815892169, 640255056),
                                    "content and mtime have changed",
                                )),
                            );

                            d.add_leaf(
                                "mtime-edit-symlink",
                                Some(FSNode::symlink((1245890614, 586345017), "fixed/path")),
                                Some(FSNode::symlink((692432309, 274032817), "fixed/path")),
                            );
                            d.add_leaf(
                                "endpoint-edit-symlink",
                                Some(FSNode::symlink((1397507428, 322887183), "changed/path")),
                                Some(FSNode::symlink((1397507428, 322887183), "path/did/change")),
                            );
                            d.add_leaf(
                                "both-edit-symlink",
                                Some(FSNode::symlink(
                                    (547336108, 262002124),
                                    "changed/path/and/mtime",
                                )),
                                Some(FSNode::symlink(
                                    (1274175004, 906839206),
                                    "path/and/mtime/did/change/",
                                )),
                            );

                            d.add_leaf(
                                "file-to-dir",
                                Some(FSNode::file((547336108, 262002124), "mock content")),
                                Some(FSNode::dir((1364181678, 477789959), mock_dir_content)),
                            );
                            d.add_leaf(
                                "file-to-symlink",
                                Some(FSNode::file((1396993467, 652868396), "fake content")),
                                Some(FSNode::symlink((616276621, 81572476), "fake/new/path")),
                            );
                            d.add_leaf(
                                "symlink-to-file",
                                Some(FSNode::symlink((776329648, 625499475), "random/path")),
                                Some(FSNode::file((1538377515, 691983830), "this is file")),
                            );
                            d.add_leaf(
                                "symlink-to-dir",
                                Some(FSNode::symlink((1493041434, 347743433), "fake/path")),
                                Some(FSNode::dir((1364181678, 477789959), mock_dir_content)),
                            );
                            d.add_leaf(
                                "dir-to-file",
                                Some(FSNode::dir((1364181678, 477789959), mock_dir_content)),
                                Some(FSNode::file((1602277549, 958804909), "not dir anymore")),
                            );
                            d.add_leaf(
                                "dir-to-symlink",
                                Some(FSNode::dir((1364181678, 477789959), mock_dir_content)),
                                Some(FSNode::symlink((1122800412, 992618853), "also/not/dir")),
                            );
                        },
                    )
                },
            );
        });

        assert_eq!(supposed_delta, get_delta(&pre_fstree, &post_fstree));

        let mut fstree_to_upgrade = pre_fstree.clone();
        fstree_to_upgrade.apply_delta(&supposed_delta).unwrap();
        assert_eq!(fstree_to_upgrade, post_fstree);

        let mut fstree_to_downgrade = post_fstree;
        fstree_to_downgrade.undo_delta(&supposed_delta).unwrap();
        assert_eq!(fstree_to_downgrade, pre_fstree);
    }
}
