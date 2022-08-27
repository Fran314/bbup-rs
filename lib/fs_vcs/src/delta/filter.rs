use super::{hash_tree, AbstPath, Delta, DeltaNode, ExcludeList, FSNode, FSTree};

impl FSTree {
    fn filter_out_rec(&mut self, rel_path: &AbstPath, exclude_list: &ExcludeList) {
        let FSTree(tree) = self;
        tree.retain(|name, child| match child {
            FSNode::File(_, _) => !exclude_list.should_exclude(&rel_path.add_last(name), false),
            FSNode::SymLink(_, _) => !exclude_list.should_exclude(&rel_path.add_last(name), false),
            FSNode::Dir(_, hash, subtree) => {
                if exclude_list.should_exclude(&rel_path.add_last(name), true) {
                    return false;
                }
                subtree.filter_out_rec(&rel_path.add_last(name), exclude_list);
                *hash = hash_tree(subtree);
                true
            }
        });
    }
}

impl Delta {
    // TODO maybe these should return something about what they have filtered out?
    pub fn filter_out(&mut self, exclude_list: &ExcludeList) {
        self.filter_out_rec(&AbstPath::single("."), exclude_list);
    }
    fn filter_out_rec(&mut self, rel_path: &AbstPath, exclude_list: &ExcludeList) {
        let Delta(tree) = self;
        for (name, child) in tree {
            match child {
                DeltaNode::Leaf(pre, post) => {
                    let is_pre_dir = if let Some(FSNode::Dir(_, hash, subtree)) = pre {
                        subtree.filter_out_rec(&rel_path.add_last(name), exclude_list);
                        *hash = hash_tree(subtree);
                        true
                    } else {
                        false
                    };
                    if exclude_list.should_exclude(&rel_path.add_last(name), is_pre_dir) {
                        *pre = None;
                    }

                    let is_post_dir = if let Some(FSNode::Dir(_, hash, subtree)) = post {
                        subtree.filter_out_rec(&rel_path.add_last(name), exclude_list);
                        *hash = hash_tree(subtree);
                        true
                    } else {
                        false
                    };
                    if exclude_list.should_exclude(&rel_path.add_last(name), is_post_dir) {
                        *post = None;
                    }
                }
                DeltaNode::Branch(optm, subdelta) => {
                    if exclude_list.should_exclude(&rel_path.add_last(name), true) {
                        // Make it so that the branch will be removed once the
                        //	delta gets shaken at the end of the function
                        *optm = None;
                        *subdelta = Delta::empty();
                    } else {
                        subdelta.filter_out_rec(&rel_path.add_last(name), exclude_list);
                    }
                }
            }
        }
        self.shake();
    }
}

#[cfg(test)]
mod tests {
    use crate::FSTree;

    use super::{Delta, ExcludeList, FSNode};

    #[test]
    fn test() {
        let exclude_list = ExcludeList::from(&vec![
            String::from("name1/"),
            String::from("name2"),
            String::from("\\./name3"),
        ])
        .unwrap();

        let unfiltered_delta_gen = |pre: Option<FSNode>, post: Option<FSNode>| {
            let mut delta = Delta::empty();
            delta.add_leaf("name1", pre.clone(), post.clone());
            delta.add_leaf("name2", pre.clone(), post.clone());
            delta.add_leaf("name3", pre.clone(), post.clone());
            delta.add_leaf("name4", pre.clone(), post.clone());
            delta.add_branch("deep", None, |d| {
                d.add_leaf("name1", pre.clone(), post.clone());
                d.add_leaf("name2", pre.clone(), post.clone());
                d.add_leaf("name3", pre.clone(), post.clone());
                d.add_leaf("name4", pre.clone(), post.clone());
            });
            delta
        };

        // non-dir states
        let old_file = FSNode::file((1443314904, 885035178), "some content");
        let new_file = FSNode::file((1420465793, 406504192), "other content");
        let old_symlink = FSNode::symlink((1443314904, 885035178), "some/fake/path/");
        let new_symlink = FSNode::symlink((1420465793, 406504192), "other/fake/path");
        let non_dir_leaves = [
            (Some(old_file.clone()), None),
            (None, Some(new_file.clone())),
            (Some(old_file), Some(new_file.clone())),
            (Some(old_symlink.clone()), None),
            (None, Some(new_symlink.clone())),
            (Some(old_symlink), Some(new_symlink.clone())),
        ];
        for (pre, post) in non_dir_leaves {
            let mut unfiltered_delta = unfiltered_delta_gen(pre.clone(), post.clone());
            let supposed_filtered_delta = Delta::gen_from(|d| {
                d.add_leaf("name1", pre.clone(), post.clone());
                d.add_leaf("name4", pre.clone(), post.clone());
                d.add_branch("deep", None, |d| {
                    d.add_leaf("name1", pre.clone(), post.clone());
                    d.add_leaf("name3", pre.clone(), post.clone());
                    d.add_leaf("name4", pre.clone(), post.clone());
                });
            });
            unfiltered_delta.filter_out(&exclude_list);
            assert_eq!(unfiltered_delta, supposed_filtered_delta);
        }

        // dir & None
        let unfiltered_tree_gen = |subtree: &mut FSTree| {
            subtree.add_dir("file", (1512376465, 64263102), |t| {
                t.add_file("name1", (1443314904, 885035178), "some content");
                t.add_file("name2", (1443314904, 885035178), "some content");
                t.add_file("name3", (1443314904, 885035178), "some content");
                t.add_file("name4", (1443314904, 885035178), "some content");
            });
            subtree.add_dir("symlink", (1512376465, 64263102), |t| {
                t.add_symlink("name1", (1443314904, 885035178), "some/fake/path/");
                t.add_symlink("name2", (1443314904, 885035178), "some/fake/path/");
                t.add_symlink("name3", (1443314904, 885035178), "some/fake/path/");
                t.add_symlink("name4", (1443314904, 885035178), "some/fake/path/");
            });
            subtree.add_dir("dir", (1512376465, 64263102), |t| {
                t.add_empty_dir("name1", (1287750559, 427846972));
                t.add_empty_dir("name2", (1287750559, 427846972));
                t.add_empty_dir("name3", (1287750559, 427846972));
                t.add_empty_dir("name4", (1287750559, 427846972));
            });
            subtree.add_empty_dir("name1", (1287750559, 427846972));
            subtree.add_empty_dir("name2", (1287750559, 427846972));
            subtree.add_empty_dir("name3", (1287750559, 427846972));
            subtree.add_empty_dir("name4", (1287750559, 427846972));
        };
        let filtered_tree_gen = |subtree: &mut FSTree| {
            subtree.add_dir("file", (1512376465, 64263102), |t| {
                t.add_file("name1", (1443314904, 885035178), "some content");
                t.add_file("name3", (1443314904, 885035178), "some content");
                t.add_file("name4", (1443314904, 885035178), "some content");
            });
            subtree.add_dir("symlink", (1512376465, 64263102), |t| {
                t.add_symlink("name1", (1443314904, 885035178), "some/fake/path/");
                t.add_symlink("name3", (1443314904, 885035178), "some/fake/path/");
                t.add_symlink("name4", (1443314904, 885035178), "some/fake/path/");
            });
            subtree.add_dir("dir", (1512376465, 64263102), |t| {
                t.add_empty_dir("name3", (1287750559, 427846972));
                t.add_empty_dir("name4", (1287750559, 427846972));
            });
            subtree.add_empty_dir("name3", (1287750559, 427846972));
            subtree.add_empty_dir("name4", (1287750559, 427846972));
        };

        // dir to None
        {
            let unfiltered_dir = FSNode::dir((1512376465, 64263102), unfiltered_tree_gen);
            let filtered_dir = FSNode::dir((1512376465, 64263102), filtered_tree_gen);

            let mut unfiltered_delta = unfiltered_delta_gen(Some(unfiltered_dir), None);
            let supposed_filtered_delta = Delta::gen_from(|d| {
                d.add_leaf("name4", Some(filtered_dir.clone()), None);
                d.add_branch("deep", None, |d| {
                    d.add_leaf("name3", Some(filtered_dir.clone()), None);
                    d.add_leaf("name4", Some(filtered_dir.clone()), None);
                });
            });
            unfiltered_delta.filter_out(&exclude_list);
            assert_eq!(unfiltered_delta, supposed_filtered_delta);
        }
        // None to dir
        {
            let unfiltered_dir = FSNode::dir((1512376465, 64263102), unfiltered_tree_gen);
            let filtered_dir = FSNode::dir((1512376465, 64263102), filtered_tree_gen);

            let mut unfiltered_delta = unfiltered_delta_gen(None, Some(unfiltered_dir));
            let supposed_filtered_delta = Delta::gen_from(|d| {
                d.add_leaf("name4", None, Some(filtered_dir.clone()));
                d.add_branch("deep", None, |d| {
                    d.add_leaf("name3", None, Some(filtered_dir.clone()));
                    d.add_leaf("name4", None, Some(filtered_dir.clone()));
                });
            });
            unfiltered_delta.filter_out(&exclude_list);
            assert_eq!(unfiltered_delta, supposed_filtered_delta);
        }

        // dir & other
        for other in [new_file, new_symlink] {
            // dir to other
            {
                let unfiltered_dir = FSNode::dir((1512376465, 64263102), unfiltered_tree_gen);
                let filtered_dir = FSNode::dir((1512376465, 64263102), filtered_tree_gen);

                let mut unfiltered_delta =
                    unfiltered_delta_gen(Some(unfiltered_dir), Some(other.clone()));
                let supposed_filtered_delta = Delta::gen_from(|d| {
                    d.add_leaf("name1", None, Some(other.clone()));
                    d.add_leaf("name4", Some(filtered_dir.clone()), Some(other.clone()));
                    d.add_branch("deep", None, |d| {
                        d.add_leaf("name1", None, Some(other.clone()));
                        d.add_leaf("name3", Some(filtered_dir.clone()), Some(other.clone()));
                        d.add_leaf("name4", Some(filtered_dir.clone()), Some(other.clone()));
                    });
                });
                unfiltered_delta.filter_out(&exclude_list);
                assert_eq!(unfiltered_delta, supposed_filtered_delta);
            }

            // other to dir
            {
                let unfiltered_dir = FSNode::dir((1512376465, 64263102), unfiltered_tree_gen);
                let filtered_dir = FSNode::dir((1512376465, 64263102), filtered_tree_gen);

                let mut unfiltered_delta =
                    unfiltered_delta_gen(Some(other.clone()), Some(unfiltered_dir));
                let supposed_filtered_delta = Delta::gen_from(|d| {
                    d.add_leaf("name1", Some(other.clone()), None);
                    d.add_leaf("name4", Some(other.clone()), Some(filtered_dir.clone()));
                    d.add_branch("deep", None, |d| {
                        d.add_leaf("name1", Some(other.clone()), None);
                        d.add_leaf("name3", Some(other.clone()), Some(filtered_dir.clone()));
                        d.add_leaf("name4", Some(other.clone()), Some(filtered_dir.clone()));
                    });
                });
                unfiltered_delta.filter_out(&exclude_list);
                assert_eq!(unfiltered_delta, supposed_filtered_delta);
            }
        }

        // branch
        {
            let premtime = (1395328184, 869950727);
            let postmtime = (1396487263, 534084134);
            let mut unfiltered_delta = Delta::gen_from(|d| {
                d.add_empty_branch("name1", Some((premtime, postmtime)));
                d.add_empty_branch("name2", Some((premtime, postmtime)));
                d.add_empty_branch("name3", Some((premtime, postmtime)));
                d.add_empty_branch("name4", Some((premtime, postmtime)));
                d.add_branch("deep", None, |d| {
                    d.add_empty_branch("name1", Some((premtime, postmtime)));
                    d.add_empty_branch("name2", Some((premtime, postmtime)));
                    d.add_empty_branch("name3", Some((premtime, postmtime)));
                    d.add_empty_branch("name4", Some((premtime, postmtime)));
                });
            });
            let supposed_filtered_delta = Delta::gen_from(|d| {
                d.add_empty_branch("name4", Some((premtime, postmtime)));
                d.add_branch("deep", None, |d| {
                    d.add_empty_branch("name3", Some((premtime, postmtime)));
                    d.add_empty_branch("name4", Some((premtime, postmtime)));
                });
            });

            unfiltered_delta.filter_out(&exclude_list);
            assert_eq!(unfiltered_delta, supposed_filtered_delta);
        }
    }
}
