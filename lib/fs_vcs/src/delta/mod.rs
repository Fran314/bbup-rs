use super::{hash_tree, ExcludeList, FSNode, FSTree};
use abst_fs::{AbstPath, Mtime};
use ior::{union, IOr};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod apply;
mod filter;
mod merge;
mod rebase;

pub use apply::InapplicableDelta;
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
pub struct Delta(HashMap<String, DeltaNode>);

#[allow(clippy::new_without_default)]
impl Delta {
    pub fn new() -> Delta {
        Delta(HashMap::new())
    }
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    pub fn insert(&mut self, name: impl ToString, child: DeltaNode) -> Option<DeltaNode> {
        self.0.insert(name.to_string(), child)
    }
    pub fn get(&self, name: impl ToString) -> Option<&DeltaNode> {
        self.0.get(&name.to_string())
    }
    pub fn values_mut(&mut self) -> std::collections::hash_map::ValuesMut<String, DeltaNode> {
        self.0.values_mut()
    }
    pub fn retain(&mut self, filter: impl FnMut(&String, &mut DeltaNode) -> bool) {
        self.0.retain(filter)
    }
    pub fn entry(&mut self, e: String) -> std::collections::hash_map::Entry<String, DeltaNode> {
        self.0.entry(e)
    }

    pub fn shake(&mut self) {
        use DeltaNode::*;

        for entry in self.values_mut() {
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
        self.retain(|_, child| match child {
            Leaf(pre, post) => pre != post,
            Branch((premtime, postmtime), subdelta) => {
                premtime != postmtime || (!subdelta.is_empty())
            }
        });
    }
}

/// IntoIterator implementation for Delta
/// Note: despite Delta being a wrapper for an hashmap which usually iterates
/// on its content in random order, Delta is guaranteed to be iterated
/// alphabetically
impl IntoIterator for Delta {
    type Item = (String, DeltaNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.into_iter().collect::<Vec<(String, DeltaNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}
/// IntoIterator implementation for &Delta
/// Note: despite Delta being a wrapper for an hashmap which usually iterates
/// on its content in random order, Delta is guaranteed to be iterated
/// alphabetically
impl<'a> IntoIterator for &'a Delta {
    type Item = (&'a String, &'a DeltaNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.iter().collect::<Vec<(&String, &DeltaNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}
/// IntoIterator implementation for &Delta
/// Note: despite Delta being a wrapper for an hashmap which usually iterates
/// on its content in random order, Delta is guaranteed to be iterated
/// alphabetically
impl<'a> IntoIterator for &'a mut Delta {
    type Item = (&'a String, &'a mut DeltaNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self
            .0
            .iter_mut()
            .collect::<Vec<(&String, &mut DeltaNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}

pub fn get_delta(last_known_fstree: &FSTree, new_tree: &FSTree) -> Delta {
    use FSNode::*;
    let mut delta = Delta::new();

    for (key, ior) in union(last_known_fstree.inner(), new_tree.inner()) {
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
                            false => Delta::new(),
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

    delta
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
            let mut subdelta = Delta::new();
            subdelta_gen(&mut subdelta);
            DeltaNode::Branch(mtimes, subdelta)
        }
        pub fn empty_branch(
            ((presec, prenano), (postsec, postnano)): ((i64, u32), (i64, u32)),
        ) -> DeltaNode {
            let mtimes = (Mtime::from(presec, prenano), Mtime::from(postsec, postnano));
            DeltaNode::Branch(mtimes, Delta::new())
        }
    }

    impl Delta {
        pub fn gen_from(gen: impl Fn(&mut Delta)) -> Delta {
            let mut delta = Delta::new();
            gen(&mut delta);
            delta
        }
        pub fn add_leaf(&mut self, name: impl ToString, pre: Option<FSNode>, post: Option<FSNode>) {
            self.0.insert(name.to_string(), DeltaNode::leaf(pre, post));
        }
        pub fn add_branch(
            &mut self,
            name: impl ToString,
            mtimes: ((i64, u32), (i64, u32)),
            subdelta_gen: impl Fn(&mut Delta),
        ) {
            self.0
                .insert(name.to_string(), DeltaNode::branch(mtimes, subdelta_gen));
        }
        pub fn add_empty_branch(&mut self, name: impl ToString, mtimes: ((i64, u32), (i64, u32))) {
            self.0
                .insert(name.to_string(), DeltaNode::empty_branch(mtimes));
        }
    }

    #[test]
    fn test_delta_node_impl() {
        let dir = FSNode::empty_dir((1664594150, 542369486));
        let file = FSNode::file((1664644025, 939049994), "content");
        let symlink = FSNode::symlink((1664668347, 992255623), "path/to/somewhere");

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

    #[test]
    fn test_delta_empty() {
        assert_eq!(Delta::new(), Delta(HashMap::from([])));
        assert!(Delta::new().is_empty());

        assert!(!Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                None,
                Some(FSNode::file((1664680553, 751496381), "content")),
            );
        })
        .is_empty());
    }

    #[test]
    fn test_delta_shake() {
        let mut unshaken_delta = Delta::gen_from(|d| {
            d.add_empty_branch(
                "branch-1",
                ((1664705868, 929253071), (1664705868, 929253071)),
            );
            d.add_branch(
                "branch-2",
                ((1664747701, 103956932), (1664747701, 103956932)),
                |d| {
                    d.add_leaf("leaf-1", None, None);
                    d.add_leaf(
                        "leaf-2",
                        Some(FSNode::empty_dir((1664768608, 801663767))),
                        Some(FSNode::empty_dir((1664768608, 801663767))),
                    );
                    d.add_leaf(
                        "leaf-3",
                        Some(FSNode::file((1664802672, 795514), "content 0")),
                        Some(FSNode::file((1664802672, 795514), "content 0")),
                    );
                    d.add_leaf(
                        "leaf-4",
                        Some(FSNode::symlink((1664830393, 665533664), "path/to/0")),
                        Some(FSNode::symlink((1664830393, 665533664), "path/to/0")),
                    );
                    d.add_empty_branch(
                        "branch",
                        ((1664866042, 747401141), (1664866042, 747401141)),
                    );
                },
            );
            d.add_leaf(
                "leaf",
                Some(FSNode::dir((1664876366, 977774592), |t| {
                    t.add_file("file-1", (1664905600, 230037082), "content 1");
                })),
                Some(FSNode::dir((1664935601, 700740412), |t| {
                    t.add_file("file-2", (1664916387, 189221502), "content 2");
                })),
            );
        });

        let shaken_delta = Delta::gen_from(|d| {
            d.add_branch(
                "leaf",
                ((1664876366, 977774592), (1664935601, 700740412)),
                |d| {
                    d.add_leaf(
                        "file-1",
                        Some(FSNode::file((1664905600, 230037082), "content 1")),
                        None,
                    );
                    d.add_leaf(
                        "file-2",
                        None,
                        Some(FSNode::file((1664916387, 189221502), "content 2")),
                    );
                },
            );
        });

        unshaken_delta.shake();
        assert_eq!(unshaken_delta, shaken_delta);
    }

    #[test]
    fn test_get_delta() {
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

        let supposed_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "removed-file",
                Some(FSNode::file((1664844899, 952880867), "content 3")),
                None,
            );
            d.add_leaf(
                "removed-symlink",
                Some(FSNode::symlink((1664866516, 799949137), "path/to/3")),
                None,
            );
            d.add_leaf(
                "removed-dir",
                Some(FSNode::dir((1665080760, 125873632), |t| {
                    t.add_file("some-file", (1664904983, 225354006), "content 4");
                    t.add_symlink("some-symlink", (1664931292, 707636324), "path/to/4");
                    t.add_dir("some-dir", (1665051015, 728013427), |t| {
                        t.add_file("some-subsfile", (1664969116, 618383875), "content 5");
                        t.add_symlink("some-subsymlink", (1665009784, 973406400), "path/to/5");
                        t.add_empty_dir("some-subdir", (1665020782, 554599289));
                    });
                })),
                None,
            );

            d.add_leaf(
                "added-file",
                None,
                Some(FSNode::file((1667291618, 49665399), "content 22")),
            );
            d.add_leaf(
                "added-symlink",
                None,
                Some(FSNode::symlink((1667299371, 392444127), "path/to/22")),
            );
            d.add_leaf(
                "added-dir",
                None,
                Some(FSNode::dir((1667458204, 617921196), |t| {
                    t.add_file("some-file", (1667344231, 62151406), "content 23");
                    t.add_symlink("some-symlink", (1667386471, 512939450), "path/to/23");
                    t.add_dir("some-dir", (1667452610, 239738758), |t| {
                        t.add_file("some-subsfile", (1667413109, 643123620), "content 24");
                        t.add_symlink("some-subsymlink", (1667430861, 703560783), "path/to/24");
                        t.add_empty_dir("some-subdir", (1667436674, 904022684));
                    });
                })),
            );

            d.add_leaf(
                "mtime-edit-file",
                Some(FSNode::file((1665128681, 479153113), "content 6")),
                Some(FSNode::file((1667491403, 52601873), "content 6")),
            );
            d.add_leaf(
                "mtime-edit-symlink",
                Some(FSNode::symlink((1665223326, 633517793), "path/to/6")),
                Some(FSNode::symlink((1667512489, 728838837), "path/to/6")),
            );
            d.add_empty_branch(
                "mtime-edit-dir",
                ((1665231730, 757614747), (1667527639, 27312686)),
            );

            d.add_leaf(
                "content-edit-file",
                Some(FSNode::file((1665403797, 984813446), "content 9")),
                Some(FSNode::file((1665403797, 984813446), "content 25")),
            );
            d.add_leaf(
                "content-edit-symlink",
                Some(FSNode::symlink((1665492280, 294042651), "path/to/9")),
                Some(FSNode::symlink((1665492280, 294042651), "path/to/25")),
            );
            d.add_branch(
                "content-edit-dir",
                ((1665653391, 583942877), (1665653391, 583942877)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1665537545, 274720731), "content 10")),
                        None,
                    );
                    d.add_leaf(
                        "old-symlink",
                        Some(FSNode::symlink((1665578089, 400706450), "path/to/10")),
                        None,
                    );
                    d.add_leaf(
                        "old-dir",
                        Some(FSNode::dir((1665616031, 855387955), |t| {
                            t.add_file("some-file", (1665593626, 191212804), "content 11");
                            t.add_symlink("some-symlink", (1665602011, 364167939), "path/to/11");
                            t.add_empty_dir("some-dir", (1665609609, 381366620));
                        })),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1667565223, 544854425), "content 26")),
                    );
                    d.add_leaf(
                        "new-symlink",
                        None,
                        Some(FSNode::symlink((1667606671, 756872113), "path/to/26")),
                    );
                    d.add_leaf(
                        "new-dir",
                        None,
                        Some(FSNode::dir((1667690533, 790228724), |t| {
                            t.add_file("some-file", (1667654446, 52879214), "content 27");
                            t.add_symlink("some-symlink", (1667660746, 340510588), "path/to/27");
                            t.add_empty_dir("some-dir", (1667666855, 573555324));
                        })),
                    );
                },
            );

            d.add_leaf(
                "both-edit-file",
                Some(FSNode::file((1665658948, 294056682), "content 12")),
                Some(FSNode::file((1667700076, 989692858), "content 28")),
            );
            d.add_leaf(
                "both-edit-symlink",
                Some(FSNode::symlink((1665706590, 498424292), "path/to/12")),
                Some(FSNode::symlink((1667744237, 161786498), "path/to/28")),
            );
            d.add_branch(
                "both-edit-dir",
                ((1665857459, 273562674), (1667979024, 483039443)),
                |d| {
                    d.add_leaf(
                        "old-file",
                        Some(FSNode::file((1665721719, 759507069), "content 13")),
                        None,
                    );
                    d.add_leaf(
                        "old-symlink",
                        Some(FSNode::symlink((1665742729, 864183276), "path/to/13")),
                        None,
                    );
                    d.add_leaf(
                        "old-dir",
                        Some(FSNode::dir((1665823151, 430141738), |t| {
                            t.add_file("some-file", (1665753800, 479487453), "content 14");
                            t.add_symlink("some-symlink", (1665799314, 73687095), "path/to/14");
                            t.add_empty_dir("some-dir", (1665816185, 637073506));
                        })),
                        None,
                    );
                    d.add_leaf(
                        "new-file",
                        None,
                        Some(FSNode::file((1667786823, 846244395), "content 29")),
                    );
                    d.add_leaf(
                        "new-symlink",
                        None,
                        Some(FSNode::symlink((1667827505, 675050268), "path/to/29")),
                    );
                    d.add_leaf(
                        "new-dir",
                        None,
                        Some(FSNode::dir((1667971390, 870864659), |t| {
                            t.add_file("some-file", (1667868245, 278758645), "content 30");
                            t.add_symlink("some-symlink", (1667907662, 970681147), "path/to/30");
                            t.add_empty_dir("some-dir", (1667932481, 458833587));
                        })),
                    );
                },
            );
            d.add_leaf(
                "file-to-symlink",
                Some(FSNode::file((1665878934, 122842597), "content 15")),
                Some(FSNode::symlink((1667989833, 799488495), "path/to/31")),
            );
            d.add_leaf(
                "symlink-to-file",
                Some(FSNode::symlink((1665925952, 816940720), "path/to/15")),
                Some(FSNode::file((1668019367, 534284745), "content 31")),
            );
            d.add_leaf(
                "file-to-dir",
                Some(FSNode::file((1665952861, 367324405), "content 16")),
                Some(FSNode::dir((1668255717, 149282922), |t| {
                    t.add_file("some-file", (1668066544, 615520517), "content 32");
                    t.add_symlink("some-symlink", (1668116001, 308689102), "path/to/32");
                    t.add_dir("some-dir", (1668214742, 157637364), |t| {
                        t.add_file("some-subfile", (1668131352, 951864648), "content 33");
                        t.add_symlink("some-subsymlink", (1668149860, 566666057), "path/to/33");
                        t.add_empty_dir("some-subdir", (1668187711, 556826003));
                    });
                })),
            );
            d.add_leaf(
                "dir-to-file",
                Some(FSNode::dir((1666112742, 844333980), |t| {
                    t.add_file("some-file", (1665980032, 483481851), "content 17");
                    t.add_symlink("some-symlink", (1665989441, 197429024), "path/to/17");
                    t.add_dir("some-dir", (1666091840, 265768979), |t| {
                        t.add_file("some-subfile", (1666003479, 80356802), "content 18");
                        t.add_symlink("some-subsymlink", (1666009206, 612314999), "path/to/18");
                        t.add_empty_dir("some-subdir", (1666057999, 808033458))
                    });
                })),
                Some(FSNode::file((1668348923, 385859136), "content 34")),
            );
            d.add_leaf(
                "symlink-to-dir",
                Some(FSNode::symlink((1666150895, 596092504), "path/to/19")),
                Some(FSNode::dir((1668649280, 308757064), |t| {
                    t.add_file("some-file", (1668452197, 126511533), "content 35");
                    t.add_symlink("some-symlink", (1668491214, 884187985), "path/to/35");
                    t.add_dir("some-dir", (1668612644, 635406011), |t| {
                        t.add_file("some-subfile", (1668531025, 526845175), "content 36");
                        t.add_symlink("some-subsymlink", (1668541084, 634088395), "path/to/36");
                        t.add_empty_dir("some-subdir", (1668566846, 601299229));
                    });
                })),
            );
            d.add_leaf(
                "dir-to-symlink",
                Some(FSNode::dir((1666619883, 311193088), |t| {
                    t.add_file("some-file", (1666160237, 675128780), "content 20");
                    t.add_symlink("some-symlink", (1666226534, 830436513), "path/to/20");
                    t.add_dir("some-dir", (1666556719, 684833087), |t| {
                        t.add_file("some-subfile", (1666307759, 331079248), "content 21");
                        t.add_symlink("some-subsymlink", (1666367800, 117412925), "path/to/21");
                        t.add_empty_dir("some-subdir", (1666467991, 57155305));
                    });
                })),
                Some(FSNode::symlink((1668676395, 805654992), "path/to/37")),
            )
        });

        assert_eq!(get_delta(&pre_fstree, &post_fstree), supposed_delta);

        let mut fstree_to_upgrade = pre_fstree.clone();
        fstree_to_upgrade.apply_delta(&supposed_delta).unwrap();
        assert_eq!(fstree_to_upgrade, post_fstree);

        let mut fstree_to_downgrade = post_fstree;
        fstree_to_downgrade.undo_delta(&supposed_delta).unwrap();
        assert_eq!(fstree_to_downgrade, pre_fstree);
    }
}
