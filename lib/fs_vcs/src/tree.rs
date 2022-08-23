use super::ExcludeList;

use abst_fs::{self as fs, AbstPath, Mtime, ObjectType};
use hasher::Hash;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::collections::HashMap;

#[derive(Error, Debug, PartialEq)]
pub enum FSTreeError {
    #[error(
        "File System Tree Error: could not generate fs-tree from specified path as it is not a directory\npath: {path}"
    )]
    NonDirEntryPoint { path: AbstPath },

    #[error("File System Tree Error: inner error occurred\nSource: {src}\nError: {err}")]
    Inner { src: String, err: String },

    #[error("File System Tree Error: some error occurred.\nSource: {src}\nError: {err}")]
    Generic { src: String, err: String },
}

fn inerr<S: std::string::ToString, E: std::error::Error>(src: S) -> impl Fn(E) -> FSTreeError {
    move |err: E| -> FSTreeError {
        FSTreeError::Inner {
            src: src.to_string(),
            err: err.to_string(),
        }
    }
}
fn generr<S: std::string::ToString, T: std::string::ToString>(src: S, err: T) -> FSTreeError {
    FSTreeError::Generic {
        src: src.to_string(),
        err: err.to_string(),
    }
}
fn error_context<S: std::string::ToString>(context: S) -> impl Fn(&str) -> String {
    move |failure: &str| -> String { format!("{}\nFailed to {}", context.to_string(), failure) }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FSNode {
    File(Mtime, Hash),
    SymLink(Mtime, Hash),
    Dir(Mtime, Hash, FSTree),
}
impl PartialEq for FSNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(mtime_l, hash_l), Self::File(mtime_r, hash_r))
            | (Self::SymLink(mtime_l, hash_l), Self::SymLink(mtime_r, hash_r))

            // Do not check for subtree structure: the idea is that the hash represents
            //	itself the tree structure, so the trees are equal iff the hashes are
            //	equal, hence check the hash and not the subtree
            | (Self::Dir(mtime_l, hash_l, _), Self::Dir(mtime_r, hash_r, _)) => {
                mtime_l == mtime_r && hash_l == hash_r
            }

            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FSTree(pub HashMap<String, FSNode>);
impl FSTree {
    pub fn empty() -> FSTree {
        FSTree(HashMap::new())
    }
}

/// Hash the endpoint of a symlink
fn hash_symlink(path: &AbstPath) -> Result<Hash, FSTreeError> {
    let errctx = error_context(format!("could not hash content of file at path {path}"));
    let endpoint = fs::read_link(path).map_err(inerr(errctx("read symlink's endpoint")))?;
    Ok(hasher::hash_bytes(endpoint.as_bytes()))
}
/// Hash the content of a file
fn hash_file(path: &AbstPath) -> Result<Hash, FSTreeError> {
    let errctx = error_context(format!("could not hash content of file at path {path}"));
    let content = fs::read_file(path).map_err(inerr(errctx("read file content")))?;
    hasher::hash_stream(content).map_err(inerr(errctx("hash file content")))
}
/// Hash children of a node by concatenating their names and their relative hashes
pub fn hash_tree(FSTree(tree): &FSTree) -> Hash {
    let mut sorted_children = tree.iter().collect::<Vec<(&String, &FSNode)>>();
    sorted_children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));

    let mut s: Vec<u8> = Vec::new();
    for (name, node) in sorted_children {
        // The reason why we append the hash of the name and not the name itself
        //	is to avoid unlikely but possible collisions.
        // This makes the appended blocks all the same length, which is better
        let name_hash = hasher::hash_bytes(name.as_bytes());
        s.append(&mut name_hash.to_bytes());
        match node {
            FSNode::File(mtime, hash) => {
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            FSNode::SymLink(mtime, hash) => {
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            FSNode::Dir(mtime, hash, _) => {
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
        }
    }
    hasher::hash_bytes(s)
}

/// Generate a tree representation of the content of a path specified, saving the hashes
/// at every node and leaf to be able to detect changes
pub fn generate_fstree(root: &AbstPath, exclude_list: &ExcludeList) -> Result<FSTree, FSTreeError> {
    if root.object_type() != Some(ObjectType::Dir) {
        return Err(FSTreeError::NonDirEntryPoint { path: root.clone() });
    }
    generate_fstree_rec(root, &AbstPath::single("."), exclude_list)
}

fn generate_fstree_rec(
    path: &AbstPath,
    rel_path: &AbstPath,
    exclude_list: &ExcludeList,
) -> Result<FSTree, FSTreeError> {
    let errctx = error_context(format!(
        "could not generate fstree from subtree at path {path}"
    ));
    let mut tree: HashMap<String, FSNode> = HashMap::new();

    let read_dir_instance =
        fs::list_dir_content(path).map_err(inerr(errctx("list content of dir")))?;
    for entry in read_dir_instance {
        let objec_type = entry.object_type().ok_or_else(|| {
            generr(
                errctx(format!("get type of child at path {entry}").as_str()),
                "child should exist but doesn't have a type (as if it doesn't exist)",
            )
        })?;
        let file_name = entry.file_name().ok_or_else(|| {
            generr(
                errctx(format!("get filename of child at path {entry}").as_str()),
                "child path might be ending in `..`",
            )
        })?;
        let rel_subpath = rel_path.add_last(&file_name);
        if exclude_list.should_exclude(&rel_subpath, objec_type == ObjectType::Dir) {
            continue;
        }

        let node = match objec_type {
            ObjectType::Dir => {
                let mtime = fs::get_mtime(&entry).map_err(inerr(errctx(
                    format!("get mtime of dir at path {entry}").as_str(),
                )))?;
                let subtree = generate_fstree_rec(&entry, &rel_subpath, exclude_list)?;
                let hash = hash_tree(&subtree);
                FSNode::Dir(mtime, hash, subtree)
            }
            ObjectType::File => {
                let mtime = fs::get_mtime(&entry).map_err(inerr(errctx(
                    format!("get mtime of file at path {entry}").as_str(),
                )))?;
                let hash = hash_file(&entry)
                    .map_err(inerr(errctx(format!("hash file at path {entry}").as_str())))?;
                FSNode::File(mtime, hash)
            }
            ObjectType::SymLink => {
                let mtime = fs::get_mtime(&entry).map_err(inerr(errctx(
                    format!("get mtime of symlink at path {entry}").as_str(),
                )))?;
                let hash = hash_symlink(&entry).map_err(inerr(errctx(
                    format!("hash symlink at path {entry}").as_str(),
                )))?;
                FSNode::SymLink(mtime, hash)
            }
        };

        tree.insert(file_name, node);
    }

    Ok(FSTree(tree))
}

#[cfg(test)]
mod tests {

    use super::{
        generate_fstree, generr, hash_tree, inerr, ExcludeList, FSNode, FSTree, FSTreeError,
    };
    use abst_fs::{AbstPath, Endpoint, Mtime};
    use std::collections::HashMap;
    use std::path::PathBuf;

    impl FSTree {
        fn test_default() -> FSTree {
            let file = FSNode::File(
                Mtime::from(498705660, 314159265),
                hasher::hash_bytes(b"this is some test content"),
            );
            let symlink = FSNode::SymLink(
                Mtime::from(498705720, 271828182),
                hasher::hash_bytes(Endpoint::Unix("some/path/to/somewhere".to_string()).as_bytes()),
            );
            let dir = {
                // Content
                let file1 = FSNode::File(
                    Mtime::from(498705780, 161803398),
                    hasher::hash_bytes(b"none of your business"),
                );
                let symlink1 = FSNode::SymLink(
                    Mtime::from(498705720, 271828182),
                    hasher::hash_bytes(
                        Endpoint::Unix("another/path/to/somewhere/else".to_string()).as_bytes(),
                    ),
                );
                let dir1 = FSNode::Dir(
                    Mtime::from(498705840, 141421356),
                    hash_tree(&FSTree::empty()),
                    FSTree::empty(),
                );

                // Dir creation
                let subtree = FSTree(HashMap::from([
                    (String::from("dir1"), dir1),
                    (String::from("file1"), file1),
                    (String::from("symlink1"), symlink1),
                ]));
                FSNode::Dir(
                    Mtime::from(498705900, 628318530),
                    hash_tree(&subtree),
                    subtree,
                )
            };
            FSTree(HashMap::from([
                (String::from("dir"), dir),
                (String::from("file"), file),
                (String::from("symlink"), symlink),
            ]))
        }
    }

    #[test]
    fn test() {
        errors();

        various();

        generate();
    }

    fn errors() {
        let generic_error = FSTreeError::Generic {
            src: "some source".to_string(),
            err: "some error".to_string(),
        };
        assert_eq!(generr("some source", "some error"), generic_error);
        assert_eq!(
            FSTreeError::Inner {
                src: "some source".to_string(),
                err: generic_error.to_string()
            },
            inerr("some source")(generic_error),
        );
    }

    fn various() {
        assert_eq!(FSTree::empty(), FSTree(HashMap::new()));
        assert_eq!(FSTree::test_default(), FSTree::test_default());
        assert_ne!(FSTree::empty(), FSTree::test_default());
        assert_eq!(
            FSNode::File(
                Mtime::from(498705660, 314159265),
                hasher::hash_bytes(b"this is some test content"),
            ),
            FSNode::File(
                Mtime::from(498705660, 314159265),
                hasher::hash_bytes(b"this is some test content"),
            ),
        );
        assert_ne!(
            FSNode::File(
                Mtime::from(498705660, 314159265),
                hasher::hash_bytes(b"this is some test content"),
            ),
            FSNode::File(
                Mtime::from(498705660, 314159265),
                hasher::hash_bytes(b"this is a different test content"),
            ),
        );
        assert_ne!(
            FSNode::File(
                Mtime::from(498705660, 314159265),
                hasher::hash_bytes(b"this is some test content"),
            ),
            FSNode::File(
                Mtime::from(498705660, 0),
                hasher::hash_bytes(b"this is some test content"),
            ),
        );

        // Only the hash matters
        assert_eq!(
            FSNode::Dir(
                Mtime::from(498705660, 314159265),
                hash_tree(&FSTree::empty()),
                FSTree::empty()
            ),
            FSNode::Dir(
                Mtime::from(498705660, 314159265),
                hash_tree(&FSTree::empty()),
                FSTree::test_default()
            ),
        );
    }

    fn generate() {
        let path = PathBuf::from("/tmp/bbup-test-fs_vcs-tree-generate");
        assert!(!path.exists());
        std::fs::create_dir(&path).unwrap();

        let result = std::panic::catch_unwind(|| {
            std::fs::create_dir(path.join("dir")).unwrap();
            std::fs::create_dir(path.join("dir").join("dir1")).unwrap();
            abst_fs::set_mtime(
                &AbstPath::from(path.join("dir").join("dir1")),
                &Mtime::from(498705840, 141421356),
            )
            .unwrap();
            std::fs::write(path.join("dir").join("file1"), b"none of your business").unwrap();
            abst_fs::set_mtime(
                &AbstPath::from(path.join("dir").join("file1")),
                &Mtime::from(498705780, 161803398),
            )
            .unwrap();
            std::os::unix::fs::symlink(
                "another/path/to/somewhere/else",
                path.join("dir").join("symlink1"),
            )
            .unwrap();
            abst_fs::set_mtime(
                &AbstPath::from(path.join("dir").join("symlink1")),
                &Mtime::from(498705720, 271828182),
            )
            .unwrap();
            abst_fs::set_mtime(
                &AbstPath::from(path.join("dir")),
                &Mtime::from(498705900, 628318530),
            )
            .unwrap();

            std::fs::write(path.join("file"), b"this is some test content").unwrap();
            abst_fs::set_mtime(
                &AbstPath::from(path.join("file")),
                &Mtime::from(498705660, 314159265),
            )
            .unwrap();
            std::os::unix::fs::symlink("some/path/to/somewhere", path.join("symlink")).unwrap();
            abst_fs::set_mtime(
                &AbstPath::from(path.join("symlink")),
                &Mtime::from(498705720, 271828182),
            )
            .unwrap();
            std::fs::create_dir(path.join(".bbup")).unwrap();
            std::fs::write(path.join("excluded-file"), b"this file will be excluded").unwrap();

            let exclude_list = ExcludeList::from(&vec![String::from("excluded-file")]).unwrap();

            assert_eq!(
                generate_fstree(&AbstPath::from(&path), &exclude_list).unwrap(),
                FSTree::test_default()
            );
            assert_ne!(
                generate_fstree(&AbstPath::from(&path), &ExcludeList::from(&vec![]).unwrap())
                    .unwrap(),
                FSTree::test_default()
            );
            assert!(generate_fstree(&AbstPath::from(path.join("file")), &exclude_list).is_err())
        });
        std::fs::remove_dir_all(&path).unwrap();
        assert!(result.is_ok())
    }
}
