use std::path::PathBuf;

use crate::{fs::Metadata, hash::Hash};

use super::{DeltaFSNode, DeltaFSTree, FSNode, FSTree};

pub enum Change {
    AddDir(Metadata),
    AddFile(Metadata, Hash),
    AddSymLink(Hash),
    EditDir(Metadata),
    EditFile(Option<Metadata>, Option<Hash>),
    EditSymLink(Hash),
    RemoveDir,
    RemoveFile,
    RemoveSymLink,
}

fn flatten_add_tree_rec(FSTree(tree): &FSTree, rel_path: PathBuf) -> Vec<(PathBuf, Change)> {
    use FSNode as FN;
    let mut flat_delta: Vec<(PathBuf, Change)> = Vec::new();
    for (name, child) in tree {
        let path = rel_path.join(name);
        match child {
            FN::File(metadata, hash) => {
                flat_delta.push((path, Change::AddFile(metadata.clone(), hash.clone())))
            }
            FN::SymLink(hash) => flat_delta.push((path, Change::AddSymLink(hash.clone()))),
            FN::Dir(metadata, _, subtree) => {
                flat_delta.push((path.clone(), Change::AddDir(metadata.clone())));
                flat_delta.append(&mut flatten_add_tree_rec(subtree, path));
            }
        }
    }
    flat_delta
}
fn flatten_remove_tree_rec(FSTree(tree): &FSTree, rel_path: PathBuf) -> Vec<(PathBuf, Change)> {
    use FSNode as FN;
    let mut flat_delta: Vec<(PathBuf, Change)> = Vec::new();
    for (name, child) in tree {
        let path = rel_path.join(name);
        match child {
            FN::File(_, _) => flat_delta.push((path, Change::RemoveFile)),
            FN::SymLink(_) => flat_delta.push((path, Change::RemoveSymLink)),
            FN::Dir(_, _, subtree) => {
                flat_delta.append(&mut flatten_remove_tree_rec(subtree, path.clone()));
                flat_delta.push((path, Change::RemoveDir));
            }
        }
    }
    flat_delta
}
fn flatten_rec(DeltaFSTree(tree): &DeltaFSTree, rel_path: PathBuf) -> Vec<(PathBuf, Change)> {
    use DeltaFSNode as DN;
    use FSNode as FN;
    let mut flat_delta: Vec<(PathBuf, Change)> = Vec::new();
    for (name, child) in tree {
        let path = rel_path.join(name);
        match child {
            DN::Branch(optm, subdelta) => {
                if let Some((_, post)) = optm {
                    flat_delta.push((path.clone(), Change::EditDir(post.clone())));
                }
                flat_delta.append(&mut flatten_rec(subdelta, path))
            }
            DN::Leaf(None, Some(FN::Dir(metadata, _, subtree))) => {
                flat_delta.push((path.clone(), Change::AddDir(metadata.clone())));
                flat_delta.append(&mut flatten_add_tree_rec(subtree, path));
            }
            DN::Leaf(None, Some(FN::File(metadata, hash))) => {
                flat_delta.push((path, Change::AddFile(metadata.clone(), hash.clone())));
            }
            DN::Leaf(None, Some(FN::SymLink(metadata))) => {
                flat_delta.push((path, Change::AddSymLink(metadata.clone())))
            }

            DN::Leaf(Some(FN::Dir(_, _, _)), None) => flat_delta.push((path, Change::RemoveDir)),
            DN::Leaf(Some(FN::File(_, _)), None) => flat_delta.push((path, Change::RemoveFile)),
            DN::Leaf(Some(FN::SymLink(_)), None) => flat_delta.push((path, Change::RemoveSymLink)),

            DN::Leaf(Some(FN::Dir(_, _, _)), Some(FN::Dir(_, _, _))) => {
                // TODO maybe make these errors better?
                panic!("trying to flat an unshaken delta");
            }
            DN::Leaf(Some(FN::File(m0, h0)), Some(FN::File(m1, h1))) => {
                let optm = if m0.ne(m1) { Some(m1.clone()) } else { None };
                let opth = if h0.ne(h1) { Some(h1.clone()) } else { None };
                if optm.is_some() || opth.is_some() {
                    flat_delta.push((path, Change::EditFile(optm, opth)));
                } else {
                    // TODO maybe make these errors better?
                    panic!("trying to flat an unshaken delta");
                }
            }
            DN::Leaf(Some(FN::SymLink(h0)), Some(FN::SymLink(h1))) => {
                if h0.ne(h1) {
                    flat_delta.push((path, Change::EditSymLink(h1.clone())));
                } else {
                    // TODO maybe make these errors better?
                    panic!("trying to flat an unshaken delta");
                }
            }
            DN::Leaf(Some(pre), Some(post)) => {
                match pre {
                    FN::File(_, _) => flat_delta.push((path.clone(), Change::RemoveFile)),
                    FN::SymLink(_) => flat_delta.push((path.clone(), Change::RemoveSymLink)),
                    FN::Dir(_, _, subtree) => {
                        flat_delta.append(&mut flatten_remove_tree_rec(subtree, path.clone()));
                        flat_delta.push((path.clone(), Change::RemoveDir));
                    }
                }
                match post {
                    FN::File(metadata, hash) => {
                        flat_delta.push((path, Change::AddFile(metadata.clone(), hash.clone())))
                    }
                    FN::SymLink(hash) => flat_delta.push((path, Change::AddSymLink(hash.clone()))),
                    FN::Dir(metadata, _, subtree) => {
                        flat_delta.push((path.clone(), Change::AddDir(metadata.clone())));
                        flat_delta.append(&mut flatten_add_tree_rec(subtree, path));
                    }
                }
            }
            DN::Leaf(None, None) => {
                // TODO maybe make these errors better?
                panic!("trying to flat an unshaken delta");
            }
        }
    }
    flat_delta
}
impl DeltaFSTree {
    pub fn flatten(&self) -> Vec<(PathBuf, Change)> {
        flatten_rec(self, PathBuf::from(""))
    }
}
