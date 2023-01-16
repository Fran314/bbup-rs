use super::{delta::UnmergeableDelta, Delta};

use abst_fs::AbstPath;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Commit {
    pub commit_id: String,
    pub delta: Delta,
}
impl Commit {
    const ID_LEN: usize = 64;
    pub fn base_commit() -> Commit {
        Commit {
            commit_id: Commit::gen_null_id(),
            delta: Delta::empty(),
        }
    }
    pub fn gen_null_id() -> String {
        String::from("0").repeat(Commit::ID_LEN)
    }
    fn gen_rand_id() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"0123456789abcdef";
        let mut rng = rand::thread_rng();

        (0..Commit::ID_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
    pub fn gen_valid_id() -> String {
        let null_id = Commit::gen_null_id();
        let mut id = Commit::gen_rand_id();
        while id == null_id {
            id = Commit::gen_rand_id();
        }
        id
    }
}

#[derive(Error, Debug)]
// pub struct GetUpdError(String, UnmergeableDelta);
pub enum GetUpdError {
    #[error(
        "Get Update Delta Error: the provided commit id does not exist\nprovided commit id: {0}"
    )]
    MissingId(String),

    #[error("Get Update Delta Error: Failed to get the update delta since the last known commit\nproblematic commit id: {0}\nreason: {1}")]
    MergeError(String, UnmergeableDelta),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitList(Vec<Commit>);
impl CommitList {
    pub fn base_commit_list() -> CommitList {
        CommitList(vec![Commit::base_commit()])
    }

    pub fn most_recent_commit(&self) -> &Commit {
        let CommitList(list) = self;
        // TODO
        // I'm not sure that unwrap is the best solution here. It is true that
        // the public API does not allow to create an empty Commit List (it
        // will always have at least the base commit) so this shouldn't be a
        // problem for anyone who uses this library correctly, but it still
        // doesn't feel particularly correct to have unwrap here instead of
        // returning an option
        list.last().unwrap()
    }

    pub fn push(&mut self, commit: Commit) {
        let CommitList(list) = self;
        list.push(commit);
    }

    pub fn get_update_delta(&self, endpoint: &AbstPath, lkc: String) -> Result<Delta, GetUpdError> {
        let mut output: Delta = Delta::empty();
        let CommitList(list) = self;
        for commit in list.iter().rev() {
            if commit.commit_id.eq(&lkc) {
                return Ok(output);
            }
            if let Some(delta_at_endpoint) = commit.delta.get_subdelta_tree_copy(endpoint) {
                if let Err(err) = output.merge_prec(&delta_at_endpoint) {
                    return Err(GetUpdError::MergeError(commit.commit_id.clone(), err));
                }
            }
        }
        Err(GetUpdError::MissingId(lkc))
    }
}

#[cfg(test)]
mod tests {
    use abst_fs::AbstPath;

    use super::{super::commit::GetUpdError, super::FSNode, Commit, CommitList, Delta};

    impl Clone for Commit {
        fn clone(&self) -> Self {
            Commit {
                commit_id: self.commit_id.clone(),
                delta: self.delta.clone(),
            }
        }
    }

    #[test]
    fn test_commit_impl() {
        assert_eq!(
            Commit::base_commit(),
            Commit {
                commit_id: String::from(
                    "0000000000000000000000000000000000000000000000000000000000000000"
                ),
                delta: Delta::empty()
            }
        );

        // for _ in 1..10_000 {
        //     assert_ne!(Commit::gen_valid_id(), Commit::gen_null_id());
        //
        //     // About this assert. It is technically not impossible that two ids
        //     // collide, but the thing is:
        //     //  - the probability of collision is 1/2^256 which is
        //     //    astonishingly small
        //     //  - the point of commit ids is that when actually used, no two
        //     //    commits have the same id, so while it technically is a
        //     //    problem that collision can actually happen in production,
        //     //    this test serves the purpose that such collision do not
        //     //    happen dangerously often
        //     assert_ne!(Commit::gen_rand_id(), Commit::gen_rand_id());
        // }
    }

    #[test]
    fn test_commit_list_impl() {
        let commits = vec![
            Commit {
                commit_id: String::from(
                    "3136995bc33f33be13f4fe9c4fe0b085959835c66ee73b953509837c906c449b",
                ),
                delta: Delta::gen_from(|d| {
                    d.add_leaf(
                        "file",
                        None,
                        Some(FSNode::file((1664613885, 467795123), "content 0")),
                    );
                }),
            },
            Commit {
                commit_id: String::from(
                    "87a451fe2d98999f60eac18a96535ca57c265c89c53c8d2147cea411667898da",
                ),
                delta: Delta::gen_from(|d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1664613885, 467795123), "content 0")),
                        None,
                    );
                    d.add_leaf(
                        "dir",
                        None,
                        Some(FSNode::dir((1664677236, 240677085), |t| {
                            t.add_file("file", (1664628119, 842842996), "content 1");
                            t.add_symlink("symlink", (1664651972, 886005237), "path/to/1");
                        })),
                    );
                }),
            },
            Commit {
                commit_id: String::from(
                    "0242a8be8649c30dbcacbfe855c585383b2fd86b296999b4aa3c06ecaf1788be",
                ),
                delta: Delta::gen_from(|d| {
                    d.add_leaf(
                        "empty-dir",
                        None,
                        Some(FSNode::empty_dir((1664689557, 328753144))),
                    );
                    d.add_branch(
                        "dir",
                        ((1664677236, 240677085), (1664746166, 798094920)),
                        |d| {
                            d.add_leaf(
                                "file",
                                Some(FSNode::file((1664628119, 842842996), "content 1")),
                                Some(FSNode::file((1664701260, 195404532), "content 2")),
                            );
                            d.add_leaf(
                                "symlink-2",
                                None,
                                Some(FSNode::symlink((1664735072, 211269434), "path/to/2")),
                            );
                        },
                    );
                }),
            },
            Commit {
                commit_id: String::from(
                    "5712a9bc532516ea108bfeb1d702910cef50e8bd787493bed209d3caccdce845",
                ),
                delta: Delta::gen_from(|d| {
                    d.add_leaf(
                        "object",
                        None,
                        Some(FSNode::symlink((1664753692, 298732079), "path/to/3")),
                    );
                    d.add_branch(
                        "dir",
                        ((1664746166, 798094920), (1664880232, 503504028)),
                        |d| {
                            d.add_leaf(
                                "subdir",
                                None,
                                Some(FSNode::dir((1664830390, 665227289), |t| {
                                    t.add_file("file", (1664801944, 263794023), "content 4");
                                })),
                            );
                            d.add_leaf(
                                "file",
                                Some(FSNode::file((1664701260, 195404532), "content 2")),
                                None,
                            );
                        },
                    );
                }),
            },
            Commit {
                commit_id: String::from(
                    "053cb1f12ed3018290d2eeff455f34d567403d7ddbd1ed490d91163d1918f386",
                ),
                delta: Delta::gen_from(|d| {
                    d.add_leaf(
                        "object",
                        Some(FSNode::symlink((1664753692, 298732079), "path/to/3")),
                        Some(FSNode::file((1664919518, 273741277), "content 5")),
                    );
                    d.add_branch(
                        "dir",
                        ((1664880232, 503504028), (1664880232, 503504028)),
                        |d| {
                            d.add_branch(
                                "subdir",
                                ((1664830390, 665227289), (1665001127, 738906557)),
                                |d| {
                                    d.add_leaf(
                                        "file",
                                        Some(FSNode::file((1664801944, 263794023), "content 4")),
                                        Some(FSNode::file((1664942929, 330015954), "content 6")),
                                    );
                                    d.add_leaf(
                                        "symlink",
                                        None,
                                        Some(FSNode::symlink((1664990897, 289456056), "path/to/6")),
                                    );
                                },
                            );
                            d.add_leaf(
                                "symlink",
                                Some(FSNode::symlink((1664651972, 886005237), "path/to/1")),
                                Some(FSNode::symlink((1665025736, 484323832), "path/to/7")),
                            );
                        },
                    );
                }),
            },
        ];

        let mut commit_list = CommitList::base_commit_list();
        assert_eq!(commit_list.most_recent_commit(), &Commit::base_commit());
        for commit in &commits {
            commit_list.push(commit.clone());
            assert_eq!(commit_list.most_recent_commit(), commit);
        }

        assert_eq!(
            commit_list
                .get_update_delta(
                    &AbstPath::empty(),
                    commits.last().unwrap().commit_id.clone()
                )
                .unwrap(),
            Delta::empty()
        );

        assert_eq!(
            commit_list
                .get_update_delta(
                    &AbstPath::empty(),
                    String::from(
                        "0242a8be8649c30dbcacbfe855c585383b2fd86b296999b4aa3c06ecaf1788be"
                    )
                )
                .unwrap(),
            Delta::gen_from(|d| {
                d.add_leaf(
                    "object",
                    None,
                    Some(FSNode::file((1664919518, 273741277), "content 5")),
                );
                d.add_branch(
                    "dir",
                    ((1664746166, 798094920), (1664880232, 503504028)),
                    |d| {
                        d.add_leaf(
                            "subdir",
                            None,
                            Some(FSNode::dir((1665001127, 738906557), |t| {
                                t.add_file("file", (1664942929, 330015954), "content 6");
                                t.add_symlink("symlink", (1664990897, 289456056), "path/to/6");
                            })),
                        );
                        d.add_leaf(
                            "symlink",
                            Some(FSNode::symlink((1664651972, 886005237), "path/to/1")),
                            Some(FSNode::symlink((1665025736, 484323832), "path/to/7")),
                        );
                        d.add_leaf(
                            "file",
                            Some(FSNode::file((1664701260, 195404532), "content 2")),
                            None,
                        );
                    },
                );
            })
        );

        assert_eq!(
            commit_list
                .get_update_delta(
                    &AbstPath::from("dir"),
                    String::from(
                        "0242a8be8649c30dbcacbfe855c585383b2fd86b296999b4aa3c06ecaf1788be"
                    )
                )
                .unwrap(),
            Delta::gen_from(|d| {
                d.add_leaf(
                    "subdir",
                    None,
                    Some(FSNode::dir((1665001127, 738906557), |t| {
                        t.add_file("file", (1664942929, 330015954), "content 6");
                        t.add_symlink("symlink", (1664990897, 289456056), "path/to/6");
                    })),
                );
                d.add_leaf(
                    "symlink",
                    Some(FSNode::symlink((1664651972, 886005237), "path/to/1")),
                    Some(FSNode::symlink((1665025736, 484323832), "path/to/7")),
                );
                d.add_leaf(
                    "file",
                    Some(FSNode::file((1664701260, 195404532), "content 2")),
                    None,
                );
            })
        );

        assert_eq!(
            commit_list
                .get_update_delta(
                    &AbstPath::from("dir/subdir"),
                    String::from(
                        "0242a8be8649c30dbcacbfe855c585383b2fd86b296999b4aa3c06ecaf1788be"
                    )
                )
                .unwrap(),
            Delta::gen_from(|d| {
                d.add_leaf(
                    "file",
                    None,
                    Some(FSNode::file((1664942929, 330015954), "content 6")),
                );
                d.add_leaf(
                    "symlink",
                    None,
                    Some(FSNode::symlink((1664990897, 289456056), "path/to/6")),
                );
            })
        );
    }

    #[test]
    fn test_commit_list_error() {
        // Broken commit list, unmergeable commits
        let commits = vec![
            Commit {
                commit_id: String::from(
                    "4dcb74d0be3f59ad7964944f37dc7cfdae944487c4c491ae07ac440cddee6229",
                ),
                delta: Delta::gen_from(|d| {
                    d.add_leaf(
                        "file",
                        None,
                        Some(FSNode::file((1664596872, 607558995), "content 0")),
                    );
                }),
            },
            Commit {
                commit_id: String::from(
                    "89591ef0a1d646c2e0c21b3ac211c27e79445c39e886ad99c9b2aa3bf5dbbbcb",
                ),
                delta: Delta::gen_from(|d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1664632595, 670679871), "content 1")),
                        None,
                    );
                }),
            },
        ];

        let mut commit_list = CommitList::base_commit_list();
        for commit in commits {
            commit_list.push(commit);
        }
        assert!(matches!(
            commit_list.get_update_delta(&AbstPath::empty(), Commit::gen_null_id()),
            Err(GetUpdError::MergeError(_, _))
        ));

        // Get update delta from non existing commit id
        let mut commit_list = CommitList::base_commit_list();
        commit_list.push(Commit {
            commit_id: String::from(
                "7fcfe98c4c4531dd090d256b3477b52045bfda9d36571491a9aa820e8639bcac",
            ),
            delta: Delta::empty(),
        });
        assert!(matches!(
            commit_list.get_update_delta(
                &AbstPath::empty(),
                String::from("6a68da8083509430c8804628cbead37bfd110a2ab27bcef7141b94ceeda13006")
            ),
            Err(GetUpdError::MissingId(_))
        ));
    }
}
