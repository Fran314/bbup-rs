use super::{hash_tree, AbstPath, Delta, DeltaNode, ExcludeList, FSNode, FSTree};
use abst_fs::Mtime;

impl FSTree {
    fn filter_out(&mut self, rel_path: &AbstPath, exclude_list: &ExcludeList) {
        self.retain(|name, child| match child {
            FSNode::File(_, _) => !exclude_list.should_exclude(&rel_path.add_last(name), false),
            FSNode::SymLink(_, _) => !exclude_list.should_exclude(&rel_path.add_last(name), false),
            FSNode::Dir(_, hash, subtree) => {
                if exclude_list.should_exclude(&rel_path.add_last(name), true) {
                    return false;
                }
                subtree.filter_out(&rel_path.add_last(name), exclude_list);
                *hash = hash_tree(subtree);
                true
            }
        });
    }
}

impl Delta {
    // TODO maybe these should return something about what they have filtered out?
    pub fn filter_out(&mut self, exclude_list: &ExcludeList) {
        // Recursive inner function with initialized parameters with default
        // values
        fn recursion(delta: &mut Delta, rel_path: &AbstPath, exclude_list: &ExcludeList) {
            let Delta(tree) = delta;
            for (name, child) in tree {
                match child {
                    DeltaNode::Leaf(pre, post) => {
                        let is_pre_dir = if let Some(FSNode::Dir(_, hash, subtree)) = pre {
                            subtree.filter_out(&rel_path.add_last(name), exclude_list);
                            *hash = hash_tree(subtree);
                            true
                        } else {
                            false
                        };
                        if exclude_list.should_exclude(&rel_path.add_last(name), is_pre_dir) {
                            *pre = None;
                        }

                        let is_post_dir = if let Some(FSNode::Dir(_, hash, subtree)) = post {
                            subtree.filter_out(&rel_path.add_last(name), exclude_list);
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
                            *optm = (Mtime::from(0, 0), Mtime::from(0, 0));
                            *subdelta = Delta::empty();
                        } else {
                            recursion(subdelta, &rel_path.add_last(name), exclude_list);
                        }
                    }
                }
            }
            delta.shake();
        }

        recursion(self, &AbstPath::single("."), exclude_list);
    }
}

#[cfg(test)]
mod tests {
    use crate::FSTree;

    use super::{Delta, ExcludeList, FSNode};

    #[test]
    fn test() {
        let exclude_list = ExcludeList::from(&vec![
            String::from("name-1/"),
            String::from("name-2"),
            String::from("\\./name-3"),
        ])
        .unwrap();

        let premtime = (1664616135, 178562318);
        let postmtime = (1667283442, 650876516);
        let mtimes = (premtime, postmtime);
        let unfiltered_delta_gen = |pre: Option<FSNode>, post: Option<FSNode>| {
            Delta::gen_from(|d| {
                d.add_leaf("name-1", pre.clone(), post.clone());
                d.add_leaf("name-2", pre.clone(), post.clone());
                d.add_leaf("name-3", pre.clone(), post.clone());
                d.add_leaf("name-4", pre.clone(), post.clone());
                d.add_branch("deep", mtimes, |d| {
                    d.add_leaf("name-1", pre.clone(), post.clone());
                    d.add_leaf("name-2", pre.clone(), post.clone());
                    d.add_leaf("name-3", pre.clone(), post.clone());
                    d.add_leaf("name-4", pre.clone(), post.clone());
                });
            })
        };
        let unfiltered_tree_gen = |subtree: &mut FSTree| {
            subtree.add_dir("file", premtime, |t| {
                t.add_file("name-1", premtime, "some content");
                t.add_file("name-2", premtime, "some content");
                t.add_file("name-3", premtime, "some content");
                t.add_file("name-4", premtime, "some content");
            });
            subtree.add_dir("symlink", premtime, |t| {
                t.add_symlink("name-1", premtime, "some/fake/path/");
                t.add_symlink("name-2", premtime, "some/fake/path/");
                t.add_symlink("name-3", premtime, "some/fake/path/");
                t.add_symlink("name-4", premtime, "some/fake/path/");
            });
            subtree.add_dir("dir", premtime, |t| {
                t.add_empty_dir("name-1", postmtime);
                t.add_empty_dir("name-2", postmtime);
                t.add_empty_dir("name-3", postmtime);
                t.add_empty_dir("name-4", postmtime);
            });
            subtree.add_empty_dir("name-1", postmtime);
            subtree.add_empty_dir("name-2", postmtime);
            subtree.add_empty_dir("name-3", postmtime);
            subtree.add_empty_dir("name-4", postmtime);
        };
        let filtered_tree_gen = |subtree: &mut FSTree| {
            subtree.add_dir("file", premtime, |t| {
                t.add_file("name-1", premtime, "some content");
                t.add_file("name-3", premtime, "some content");
                t.add_file("name-4", premtime, "some content");
            });
            subtree.add_dir("symlink", premtime, |t| {
                t.add_symlink("name-1", premtime, "some/fake/path/");
                t.add_symlink("name-3", premtime, "some/fake/path/");
                t.add_symlink("name-4", premtime, "some/fake/path/");
            });
            subtree.add_dir("dir", premtime, |t| {
                t.add_empty_dir("name-3", postmtime);
                t.add_empty_dir("name-4", postmtime);
            });
            subtree.add_empty_dir("name-3", postmtime);
            subtree.add_empty_dir("name-4", postmtime);
        };

        // non-dir state to non-dir state
        let old_file = FSNode::file(premtime, "some content");
        let old_symlink = FSNode::symlink(premtime, "some/fake/path/");
        let new_file = FSNode::file(postmtime, "other content");
        let new_symlink = FSNode::symlink(postmtime, "other/fake/path");
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
                d.add_leaf("name-1", pre.clone(), post.clone());
                d.add_leaf("name-4", pre.clone(), post.clone());
                d.add_branch("deep", mtimes, |d| {
                    d.add_leaf("name-1", pre.clone(), post.clone());
                    d.add_leaf("name-3", pre.clone(), post.clone());
                    d.add_leaf("name-4", pre.clone(), post.clone());
                });
            });
            unfiltered_delta.filter_out(&exclude_list);
            assert_eq!(unfiltered_delta, supposed_filtered_delta);
        }

        // dir & None
        {
            // dir to None
            {
                let unfiltered_dir = FSNode::dir(premtime, unfiltered_tree_gen);
                let filtered_dir = FSNode::dir(premtime, filtered_tree_gen);

                let mut unfiltered_delta = unfiltered_delta_gen(Some(unfiltered_dir), None);
                let supposed_filtered_delta = Delta::gen_from(|d| {
                    d.add_leaf("name-4", Some(filtered_dir.clone()), None);
                    d.add_branch("deep", mtimes, |d| {
                        d.add_leaf("name-3", Some(filtered_dir.clone()), None);
                        d.add_leaf("name-4", Some(filtered_dir.clone()), None);
                    });
                });
                unfiltered_delta.filter_out(&exclude_list);
                assert_eq!(unfiltered_delta, supposed_filtered_delta);
            }
            // None to dir
            {
                let unfiltered_dir = FSNode::dir(premtime, unfiltered_tree_gen);
                let filtered_dir = FSNode::dir(premtime, filtered_tree_gen);

                let mut unfiltered_delta = unfiltered_delta_gen(None, Some(unfiltered_dir));
                let supposed_filtered_delta = Delta::gen_from(|d| {
                    d.add_leaf("name-4", None, Some(filtered_dir.clone()));
                    d.add_branch("deep", mtimes, |d| {
                        d.add_leaf("name-3", None, Some(filtered_dir.clone()));
                        d.add_leaf("name-4", None, Some(filtered_dir.clone()));
                    });
                });
                unfiltered_delta.filter_out(&exclude_list);
                assert_eq!(unfiltered_delta, supposed_filtered_delta);
            }
        }

        // dir & other
        // The for isn't the prettiest solution but the code inside the for has
        // to be executed both with a file and with a symlink, and it would be
        // exactly the same except for the node used, so a for like this does
        // the job
        for other in [new_file, new_symlink] {
            // dir to other
            {
                let unfiltered_dir = FSNode::dir(premtime, unfiltered_tree_gen);
                let filtered_dir = FSNode::dir(premtime, filtered_tree_gen);

                let mut unfiltered_delta =
                    unfiltered_delta_gen(Some(unfiltered_dir), Some(other.clone()));
                let supposed_filtered_delta = Delta::gen_from(|d| {
                    d.add_leaf("name-1", None, Some(other.clone()));
                    d.add_leaf("name-4", Some(filtered_dir.clone()), Some(other.clone()));
                    d.add_branch("deep", mtimes, |d| {
                        d.add_leaf("name-1", None, Some(other.clone()));
                        d.add_leaf("name-3", Some(filtered_dir.clone()), Some(other.clone()));
                        d.add_leaf("name-4", Some(filtered_dir.clone()), Some(other.clone()));
                    });
                });
                unfiltered_delta.filter_out(&exclude_list);
                assert_eq!(unfiltered_delta, supposed_filtered_delta);
            }

            // other to dir
            {
                let unfiltered_dir = FSNode::dir(premtime, unfiltered_tree_gen);
                let filtered_dir = FSNode::dir(premtime, filtered_tree_gen);

                let mut unfiltered_delta =
                    unfiltered_delta_gen(Some(other.clone()), Some(unfiltered_dir));
                let supposed_filtered_delta = Delta::gen_from(|d| {
                    d.add_leaf("name-1", Some(other.clone()), None);
                    d.add_leaf("name-4", Some(other.clone()), Some(filtered_dir.clone()));
                    d.add_branch("deep", mtimes, |d| {
                        d.add_leaf("name-1", Some(other.clone()), None);
                        d.add_leaf("name-3", Some(other.clone()), Some(filtered_dir.clone()));
                        d.add_leaf("name-4", Some(other.clone()), Some(filtered_dir.clone()));
                    });
                });
                unfiltered_delta.filter_out(&exclude_list);
                assert_eq!(unfiltered_delta, supposed_filtered_delta);
            }
        }

        // branch
        {
            let mut unfiltered_delta = Delta::gen_from(|d| {
                d.add_empty_branch("name-1", (premtime, postmtime));
                d.add_empty_branch("name-2", (premtime, postmtime));
                d.add_empty_branch("name-3", (premtime, postmtime));
                d.add_empty_branch("name-4", (premtime, postmtime));
                d.add_branch("deep", (premtime, postmtime), |d| {
                    d.add_empty_branch("name-1", (premtime, postmtime));
                    d.add_empty_branch("name-2", (premtime, postmtime));
                    d.add_empty_branch("name-3", (premtime, postmtime));
                    d.add_empty_branch("name-4", (premtime, postmtime));
                });
            });
            let supposed_filtered_delta = Delta::gen_from(|d| {
                d.add_empty_branch("name-4", (premtime, postmtime));
                d.add_branch("deep", (premtime, postmtime), |d| {
                    d.add_empty_branch("name-3", (premtime, postmtime));
                    d.add_empty_branch("name-4", (premtime, postmtime));
                });
            });

            unfiltered_delta.filter_out(&exclude_list);
            assert_eq!(unfiltered_delta, supposed_filtered_delta);
        }
    }
}
