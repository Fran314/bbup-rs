use abst_fs::AbstPath;

use super::{Delta, DeltaNode, FSNode, FSTree};

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
#[error(
    "File System Tree Delta Error: unable to rebase delta at the given endpoint on the given tree.\nConflict at path: {0}\nError: {1}"
)]
pub struct UnrebasableDelta(AbstPath, String);
fn unrebaseerr(path: AbstPath, err: impl ToString) -> UnrebasableDelta {
    UnrebasableDelta(path, err.to_string())
}

impl Delta {
    pub fn rebase_from_tree_at_endpoint(
        &self,
        fstree: &FSTree,
        endpoint: &AbstPath,
    ) -> Result<Delta, UnrebasableDelta> {
        // Recursive inner function with initialized parameters with default
        // values
        fn recursion(
            delta: &Delta,
            fstree: &FSTree,
            endpoint: &AbstPath,
            relative_path: AbstPath,
        ) -> Result<Delta, UnrebasableDelta> {
            match endpoint.get(0) {
                None => Ok(delta.clone()),
                Some(component) => match fstree.get(component) {
                    Some(FSNode::Dir(mtime, _, subtree)) => {
                        let mut output = Delta::new();
                        output.insert(
                            component,
                            DeltaNode::Branch(
                                (mtime.clone(), mtime.clone()),
                                recursion(
                                    delta,
                                    subtree,
                                    &endpoint.strip_first(),
                                    relative_path.add_last(component.clone()),
                                )?,
                            ),
                        );
                        Ok(output)
                    }
                    Some(_) => Err(unrebaseerr(
                        relative_path.add_last(component.clone()),
                        "node at path is not directory",
                    )),
                    None => Err(unrebaseerr(
                        relative_path.add_last(component.clone()),
                        "node at path does not exist",
                    )),
                },
            }
        }

        recursion(self, fstree, endpoint, AbstPath::empty())
    }
}

#[cfg(test)]
mod tests {
    use abst_fs::AbstPath;

    use super::{unrebaseerr, Delta, FSNode, FSTree};

    #[test]
    fn test_rebase_from_tree_at_endpoint() {
        let fstree = FSTree::gen_from(|t| {
            t.add_dir("path", (1664762240, 128038952), |t| {
                t.add_dir("to", (1664737932, 714969377), |t| {
                    t.add_dir("somewhere", (1664691462, 245200293), |t| {
                        t.add_file("old-file", (1664599646, 145075909), "content 0");
                        t.add_file("old-symlink", (1664617566, 770172845), "path/to/0");
                        t.add_empty_dir("old-dir", (1664665740, 244894146));
                    });
                });
            });
            t.add_file("old-file", (1664599646, 145075909), "content 0");
            t.add_file("old-symlink", (1664617566, 770172845), "path/to/0");
            t.add_empty_dir("old-dir", (1664665740, 244894146));
        });

        let delta = Delta::gen_from(|d| {
            d.add_leaf(
                "old-file",
                Some(FSNode::file((1664599646, 145075909), "content 0")),
                None,
            );
            d.add_leaf(
                "old-symlink",
                Some(FSNode::symlink((1664617566, 770172845), "path/to/0")),
                None,
            );
            d.add_leaf(
                "old-dir",
                Some(FSNode::empty_dir((1664665740, 244894146))),
                None,
            );
            d.add_leaf(
                "new-file",
                None,
                Some(FSNode::file((1664789067, 991957283), "content 1")),
            );
            d.add_leaf(
                "new-symlink",
                None,
                Some(FSNode::symlink((1664796814, 684583065), "path/to/1")),
            );
            d.add_leaf(
                "new-dir",
                None,
                Some(FSNode::empty_dir((1664789067, 991957283))),
            );
        });

        assert_eq!(
            delta
                .rebase_from_tree_at_endpoint(&fstree, &AbstPath::from("path/to/somewhere"))
                .unwrap(),
            Delta::gen_from(|d| {
                d.add_branch(
                    "path",
                    ((1664762240, 128038952), (1664762240, 128038952)),
                    |d| {
                        d.add_branch(
                            "to",
                            ((1664737932, 714969377), (1664737932, 714969377)),
                            |d| {
                                d.add_branch(
                                    "somewhere",
                                    ((1664691462, 245200293), (1664691462, 245200293)),
                                    |d| {
                                        *d = delta.clone();
                                    },
                                );
                            },
                        )
                    },
                );
            })
        );

        assert_eq!(
            delta
                .rebase_from_tree_at_endpoint(&fstree, &AbstPath::empty())
                .unwrap(),
            delta
        );

        // The files edited by the delta do not exist in `./path/to` but that
        // does not matter because the only thing that rebase cares about is
        // the path of rebasing, and it doesn't look up if the delta can
        // actually be applied at the rebase endpoint
        assert_eq!(
            delta
                .rebase_from_tree_at_endpoint(&fstree, &AbstPath::from("path/to"))
                .unwrap(),
            Delta::gen_from(|d| {
                d.add_branch(
                    "path",
                    ((1664762240, 128038952), (1664762240, 128038952)),
                    |d| {
                        d.add_branch(
                            "to",
                            ((1664737932, 714969377), (1664737932, 714969377)),
                            |d| {
                                *d = delta.clone();
                            },
                        )
                    },
                );
            })
        );

        assert_eq!(
            delta.rebase_from_tree_at_endpoint(
                &fstree,
                &AbstPath::from("path/to/somewhere/old-file/dir/subdir")
            ),
            Err(unrebaseerr(
                AbstPath::from("path/to/somewhere/old-file"),
                "node at path is not directory"
            ))
        );

        assert_eq!(
            delta.rebase_from_tree_at_endpoint(
                &fstree,
                &AbstPath::from("path/to/nowhere/dir/subdir")
            ),
            Err(unrebaseerr(
                AbstPath::from("path/to/nowhere"),
                "node at path does not exist"
            ))
        );
    }
}
