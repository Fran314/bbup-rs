use super::ExcludeList;

use abst_fs::{self as fs, AbstPath, Endpoint, Mtime, ObjectType};
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
    SymLink(Mtime, Endpoint),
    Dir(Mtime, Hash, FSTree),
}
impl PartialEq for FSNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(mtime_l, hash_l), Self::File(mtime_r, hash_r))

            // Do not check for subtree structure: the idea is that the hash represents
            //	itself the tree structure, so the trees are equal iff the hashes are
            //	equal, hence check the hash and not the subtree
            | (Self::Dir(mtime_l, hash_l, _), Self::Dir(mtime_r, hash_r, _)) => {
                mtime_l == mtime_r && hash_l == hash_r
            }

            (Self::SymLink(mtime_l, endpoint_l), Self::SymLink(mtime_r, endpoint_r)) => {
                mtime_l == mtime_r && endpoint_l == endpoint_r
            }

            _ => false,
        }
    }
}
impl FSNode {
    pub fn hash_node(&self) -> Hash {
        use hasher::hash_bytes;
        let mut s: Vec<u8> = Vec::new();
        match self {
            FSNode::File(mtime, hash) => {
                s.append(&mut b"f".to_vec());
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
            FSNode::SymLink(mtime, endpoint) => {
                s.append(&mut b"s".to_vec());
                s.append(&mut mtime.to_bytes());
                // As for the name in `hash_tree`, we add the hash of the endpoint
                // and not the endpoint itself as bytes to avoid unlikely but
                // possible collisions.
                s.append(&mut hash_bytes(endpoint.as_bytes()).to_bytes());
            }
            FSNode::Dir(mtime, hash, _) => {
                s.append(&mut b"d".to_vec());
                s.append(&mut mtime.to_bytes());
                s.append(&mut hash.to_bytes());
            }
        }
        hash_bytes(s)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FSTree(HashMap<String, FSNode>);

#[allow(clippy::new_without_default)]
impl FSTree {
    pub fn inner(&self) -> &HashMap<String, FSNode> {
        &self.0
    }

    pub fn new() -> FSTree {
        FSTree(HashMap::new())
    }

    pub fn insert(&mut self, name: impl ToString, child: FSNode) -> Option<FSNode> {
        self.0.insert(name.to_string(), child)
    }

    pub fn get(&self, name: impl ToString) -> Option<&FSNode> {
        self.0.get(&name.to_string())
    }

    pub fn retain(&mut self, filter: impl FnMut(&String, &mut FSNode) -> bool) {
        self.0.retain(filter)
    }

    pub fn entry(&mut self, e: String) -> std::collections::hash_map::Entry<String, FSNode> {
        self.0.entry(e)
    }

    /// Hash children of a node by concatenating their names and their relative
    /// hashes
    pub fn hash_tree(&self) -> Hash {
        use hasher::hash_bytes;
        let mut s: Vec<u8> = Vec::new();
        for (name, node) in self {
            // The reason why we append the hash of the name and not the name
            // itself is to avoid unlikely but possible collisions.
            // This makes the appended blocks all the same length, which is
            // better
            let name_hash = hash_bytes(name.as_bytes());
            s.append(&mut name_hash.to_bytes());
            s.append(&mut node.hash_node().to_bytes());
        }
        hash_bytes(s)
    }
}

/// IntoIterator implementation for FSTree
/// Note: despite FSTree being a wrapper for an hashmap which usually iterates
/// on its content in random order, FSTree is guaranteed to be iterated
/// alphabetically
impl IntoIterator for FSTree {
    type Item = (String, FSNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.into_iter().collect::<Vec<(String, FSNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}
/// IntoIterator implementation for &FSTree
/// Note: despite FSTree being a wrapper for an hashmap which usually iterates
/// on its content in random order, FSTree is guaranteed to be iterated
/// alphabetically
impl<'a> IntoIterator for &'a FSTree {
    type Item = (&'a String, &'a FSNode);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let mut children = self.0.iter().collect::<Vec<(&String, &FSNode)>>();
        children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
        children.into_iter()
    }
}

/// Hash the content of a file
pub fn hash_file(path: &AbstPath) -> Result<Hash, FSTreeError> {
    let errctx = error_context(format!("could not hash content of file at path {path}"));
    let content = fs::read_file(path).map_err(inerr(errctx("read file content")))?;
    hasher::hash_stream(content).map_err(inerr(errctx("hash file content")))
}

/// Generate a tree representation of the content of a path specified, saving the hashes
/// at every node and leaf to be able to detect changes
pub fn generate_fstree(root: &AbstPath, exclude_list: &ExcludeList) -> Result<FSTree, FSTreeError> {
    if root.object_type() != Some(ObjectType::Dir) {
        return Err(FSTreeError::NonDirEntryPoint { path: root.clone() });
    }

    // Recursive inner function with initialized parameters with default values
    fn recursion(
        path: &AbstPath,
        rel_path: &AbstPath,
        exclude_list: &ExcludeList,
    ) -> Result<FSTree, FSTreeError> {
        let errctx = error_context(format!(
            "could not generate fstree from subtree at path {path}"
        ));
        let mut tree = FSTree::new();

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
                    let subtree = recursion(&entry, &rel_subpath, exclude_list)?;
                    let hash = subtree.hash_tree();
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
                    let endpoint = fs::read_link(&entry).map_err(inerr(errctx(
                        format!("get endpoint of symlink at path {entry}").as_str(),
                    )))?;
                    FSNode::SymLink(mtime, endpoint)
                }
            };

            tree.insert(file_name, node);
        }
        Ok(tree)
    }

    recursion(root, &AbstPath::single("."), exclude_list)
}

#[cfg(test)]
mod tests {
    use super::{generate_fstree, generr, inerr, ExcludeList, FSNode, FSTree, FSTreeError};
    use abst_fs::{AbstPath, Endpoint, Mtime};
    use std::collections::HashMap;
    use std::io::Write;
    use std::path::PathBuf;

    // --- SAFETY MEASURES --- //
    // Testing this crate is dangerous as an unexpected behaviour could
    // potentially damage the machine it's ran on. To fix this we uso two safety
    // measures: we create object and work only inside the `/tmp` folder, and
    // we make sure that the paths (AbstPath) that we use actually mean what we
    // think they mean, to avoid situations where we think we're deleting a file
    // inside /tmp but actually we're deleting a home directory.
    //
    // The purpose of `safe_add_last` is to append components to paths while
    // checking that they still mean what we think they mean.
    //
    // `setup_sandbox` and `cleanup_sandbox` create and destroy the sandbox
    // environments in which the testing will happen. The creation happens only
    // after having checked that the path didn't get corrupted while being
    // transformed into an AbstPath
    trait SafeAdd {
        fn safe_add_last<S: std::string::ToString>(&self, suffix: S) -> (AbstPath, PathBuf);
    }
    impl SafeAdd for (AbstPath, PathBuf) {
        fn safe_add_last<S: std::string::ToString>(&self, suffix: S) -> (AbstPath, PathBuf) {
            let (abst, pb) = self;
            let new_abst = abst.add_last(suffix.to_string());
            let new_pb = pb.join(suffix.to_string());
            //	make sure the path means actually what I think it mean
            assert_eq!(new_abst.to_path_buf(), new_pb);
            (new_abst, new_pb)
        }
    }
    fn setup_sandbox(path: impl std::fmt::Display) -> (AbstPath, PathBuf) {
        let path_bf = PathBuf::from(format!("/tmp/{path}"));
        assert!(!path_bf.exists());
        std::fs::create_dir(&path_bf).unwrap();

        let path = (AbstPath::from(&path_bf), path_bf);
        assert_eq!(path.0.to_path_buf(), path.1); // make sure the path means actually what I think it mean
        path
    }
    fn cleanup_sandbox(path: impl std::fmt::Display) {
        let path_bf = PathBuf::from(format!("/tmp/{path}"));
        std::fs::remove_dir_all(path_bf).unwrap();
    }
    // --- --- //

    impl FSNode {
        pub fn file(mtime: (i64, u32), content: impl ToString) -> FSNode {
            FSNode::File(
                Mtime::from(mtime.0, mtime.1),
                hasher::hash_bytes(content.to_string().as_bytes()),
            )
        }
        pub fn symlink(mtime: (i64, u32), path: impl ToString) -> FSNode {
            FSNode::SymLink(
                Mtime::from(mtime.0, mtime.1),
                Endpoint::Unix(path.to_string()),
            )
        }
        pub fn dir(mtime: (i64, u32), subtree_gen: impl Fn(&mut FSTree)) -> FSNode {
            let mut subtree = FSTree::new();
            subtree_gen(&mut subtree);
            FSNode::Dir(Mtime::from(mtime.0, mtime.1), subtree.hash_tree(), subtree)
        }
        pub fn empty_dir(mtime: (i64, u32)) -> FSNode {
            FSNode::Dir(
                Mtime::from(mtime.0, mtime.1),
                FSTree::new().hash_tree(),
                FSTree::new(),
            )
        }
    }
    impl FSTree {
        pub fn gen_from(gen: impl Fn(&mut FSTree)) -> FSTree {
            let mut tree = FSTree::new();
            gen(&mut tree);
            tree
        }

        pub fn add_file(&mut self, name: impl ToString, mtime: (i64, u32), content: impl ToString) {
            self.insert(name.to_string(), FSNode::file(mtime, content));
        }
        pub fn add_symlink(&mut self, name: impl ToString, mtime: (i64, u32), path: impl ToString) {
            self.insert(name.to_string(), FSNode::symlink(mtime, path));
        }
        pub fn add_dir(
            &mut self,
            name: impl ToString,
            mtime: (i64, u32),
            subtree_gen: impl Fn(&mut FSTree),
        ) {
            self.insert(name.to_string(), FSNode::dir(mtime, subtree_gen));
        }
        pub fn add_empty_dir(&mut self, name: impl ToString, mtime: (i64, u32)) {
            self.insert(name.to_string(), FSNode::empty_dir(mtime));
        }
    }

    #[test]
    fn test_errors() {
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

    #[test]
    fn test_node_equality() {
        assert_eq!(FSTree::new(), FSTree(HashMap::new()));
        assert_ne!(
            FSTree::new(),
            FSTree::gen_from(|t| {
                t.add_file("file", (1664650696, 234467902), "content");
            })
        );
        assert_eq!(
            FSNode::file((1664717709, 926293437), "this is some test content"),
            FSNode::file((1664717709, 926293437), "this is some test content")
        );
        assert_ne!(
            FSNode::file((1664717709, 926293437), "this is some test content"),
            FSNode::file((1664717709, 926293437), "this is a different test content")
        );
        assert_ne!(
            FSNode::file((1664717709, 926293437), "this is some test content"),
            FSNode::file((1664717709, 0), "this is some test content")
        );

        // Only mtime and hash matters, not the actual subtree
        assert_eq!(
            FSNode::empty_dir((1664996516, 439383420)),
            FSNode::Dir(
                Mtime::from(1664996516, 439383420),
                FSTree::new().hash_tree(),
                FSTree::gen_from(|t| { t.add_file("file", (1664840147, 706805147), "content") })
            ),
        );
    }

    #[test]
    // While it is not ideal to have one huge test function testing all the
    // possible behaviours, given the possibility of danger of these tests it is
    // better to execute them sequencially in a deliberate order rather than
    // in parallel or in random order. This is why the tests for this module are
    // all in one huge function
    fn test_generate() {
        let sandbox = "bbup-test-fs_vcs-tree-generate";
        let path = setup_sandbox(sandbox);

        let result = std::panic::catch_unwind(|| {
            let (file, _) = path.safe_add_last("file");
            abst_fs::create_file(&file)
                .unwrap()
                .write_all(b"content 0")
                .unwrap();
            abst_fs::set_mtime(&file, &Mtime::from(1665053062, 547894622)).unwrap();

            let (symlink, _) = path.safe_add_last("symlink");
            abst_fs::create_symlink(&symlink, Endpoint::Unix(String::from("path/to/0"))).unwrap();
            abst_fs::set_mtime(&symlink, &Mtime::from(1665170204, 69478848)).unwrap();

            let (exc_file, _) = path.safe_add_last("excluded-file");
            abst_fs::create_file(&exc_file)
                .unwrap()
                .write_all(b"content 1")
                .unwrap();
            abst_fs::set_mtime(&exc_file, &Mtime::from(1665215589, 640928345)).unwrap();

            let (exc_symlink, _) = path.safe_add_last("excluded-symlink");
            abst_fs::create_symlink(&exc_symlink, Endpoint::Unix(String::from("path/to/1")))
                .unwrap();
            abst_fs::set_mtime(&exc_symlink, &Mtime::from(1665232043, 74272520)).unwrap();

            let dir = path.safe_add_last("dir");
            abst_fs::create_dir(&dir.0).unwrap();
            {
                let (subfile, _) = dir.safe_add_last("subfile");
                abst_fs::create_file(&subfile)
                    .unwrap()
                    .write_all(b"content 2")
                    .unwrap();
                abst_fs::set_mtime(&subfile, &Mtime::from(1665270936, 217169000)).unwrap();

                let (subsymlink, _) = dir.safe_add_last("subsymlink");
                abst_fs::create_symlink(&subsymlink, Endpoint::Unix(String::from("path/to/2")))
                    .unwrap();
                abst_fs::set_mtime(&subsymlink, &Mtime::from(1665300913, 716103834)).unwrap();

                let (exc_subfile, _) = dir.safe_add_last("excluded-subfile");
                abst_fs::create_file(&exc_subfile)
                    .unwrap()
                    .write_all(b"content 3")
                    .unwrap();
                abst_fs::set_mtime(&exc_subfile, &Mtime::from(1665302711, 514622712)).unwrap();

                let (exc_subsymlink, _) = dir.safe_add_last("excluded-subsymlink");
                abst_fs::create_symlink(&exc_subsymlink, Endpoint::Unix(String::from("path/to/3")))
                    .unwrap();
                abst_fs::set_mtime(&exc_subsymlink, &Mtime::from(1665327254, 777711742)).unwrap();

                let subdir = dir.safe_add_last("subdir");
                abst_fs::create_dir(&subdir.0).unwrap();
                {
                    let (subsubfile, _) = subdir.safe_add_last("subsubfile");
                    abst_fs::create_file(&subsubfile)
                        .unwrap()
                        .write_all(b"content 4")
                        .unwrap();
                    abst_fs::set_mtime(&subsubfile, &Mtime::from(1665302711, 514622712)).unwrap();

                    let (subsubsymlink, _) = subdir.safe_add_last("subsubsymlink");
                    abst_fs::create_symlink(
                        &subsubsymlink,
                        Endpoint::Unix(String::from("path/to/4")),
                    )
                    .unwrap();
                    abst_fs::set_mtime(&subsubsymlink, &Mtime::from(1665327254, 777711742))
                        .unwrap();

                    let (subsubdir, _) = subdir.safe_add_last("subsubdir");
                    abst_fs::create_dir(&subsubdir).unwrap();
                    abst_fs::set_mtime(&subsubdir, &Mtime::from(1665390646, 304112003)).unwrap();
                }
                abst_fs::set_mtime(&subdir.0, &Mtime::from(1665541970, 8962068)).unwrap();

                let (exc_subdir, _) = dir.safe_add_last("excluded-subdir");
                abst_fs::create_dir(&exc_subdir).unwrap();
                abst_fs::set_mtime(&exc_subdir, &Mtime::from(1665589504, 816358149)).unwrap();
            }
            abst_fs::set_mtime(&dir.0, &Mtime::from(1665594782, 877788919)).unwrap();

            let (exc_dir, _) = path.safe_add_last("excluded-dir");
            abst_fs::create_dir(&exc_dir).unwrap();
            abst_fs::set_mtime(&exc_dir, &Mtime::from(1665619691, 269349348)).unwrap();

            let full_tree = FSTree::gen_from(|t| {
                t.add_file("file", (1665053062, 547894622), "content 0");
                t.add_symlink("symlink", (1665170204, 69478848), "path/to/0");
                t.add_file("excluded-file", (1665215589, 640928345), "content 1");
                t.add_symlink("excluded-symlink", (1665232043, 74272520), "path/to/1");
                t.add_empty_dir("excluded-dir", (1665619691, 269349348));

                t.add_dir("dir", (1665594782, 877788919), |t| {
                    t.add_file("subfile", (1665270936, 217169000), "content 2");
                    t.add_symlink("subsymlink", (1665300913, 716103834), "path/to/2");
                    t.add_file("excluded-subfile", (1665302711, 514622712), "content 3");
                    t.add_symlink("excluded-subsymlink", (1665327254, 777711742), "path/to/3");
                    t.add_empty_dir("excluded-subdir", (1665589504, 816358149));

                    t.add_dir("subdir", (1665541970, 8962068), |t| {
                        t.add_file("subsubfile", (1665302711, 514622712), "content 4");
                        t.add_symlink("subsubsymlink", (1665327254, 777711742), "path/to/4");
                        t.add_empty_dir("subsubdir", (1665390646, 304112003));
                    });
                });
            });

            let partial_tree = FSTree::gen_from(|t| {
                t.add_file("file", (1665053062, 547894622), "content 0");
                t.add_symlink("symlink", (1665170204, 69478848), "path/to/0");

                t.add_dir("dir", (1665594782, 877788919), |t| {
                    t.add_file("subfile", (1665270936, 217169000), "content 2");
                    t.add_symlink("subsymlink", (1665300913, 716103834), "path/to/2");

                    t.add_dir("subdir", (1665541970, 8962068), |t| {
                        t.add_file("subsubfile", (1665302711, 514622712), "content 4");
                        t.add_symlink("subsubsymlink", (1665327254, 777711742), "path/to/4");
                        t.add_empty_dir("subsubdir", (1665390646, 304112003));
                    });
                });
            });

            assert_eq!(
                generate_fstree(
                    &AbstPath::from(&path.1),
                    &ExcludeList::from(&vec![]).unwrap()
                )
                .unwrap(),
                full_tree
            );

            let exclude_list = ExcludeList::from(&vec![String::from("excluded-.*")]).unwrap();
            assert_eq!(
                generate_fstree(&AbstPath::from(&path.1), &exclude_list).unwrap(),
                partial_tree
            );
        });

        cleanup_sandbox(sandbox);

        assert!(result.is_ok())
    }
}
