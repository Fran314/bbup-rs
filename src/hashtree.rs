#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::Path;

use base64ct::{Base64, Encoding};
use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};

#[derive(PartialEq)]
pub enum Traverse {
    Pre,
    Post,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone)]
pub enum Type {
    Dir,
    File,
    Symlink,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HashTreeNode {
    pub file_name: String,
    pub nodetype: Type,
    pub hash: String,
    pub children: Vec<HashTreeNode>,
}
impl HashTreeNode {
    fn to_string(&self) -> String {
        self.file_name.clone() + self.hash.as_str()
    }
}

#[derive(Debug)]
pub struct Diff {
    pub added: Vec<(Type, String)>,
    pub edited: Vec<(Type, String)>,
    pub removed: Vec<(Type, String)>,
}

fn hash_string(s: &str) -> String {
    let hash = Sha256::digest(s);
    Base64::encode_string(&hash)
}
fn hash_children(v: &Vec<HashTreeNode>) -> String {
    let mut s = String::new();
    for child in v {
        s += child.to_string().as_str();
    }
    hash_string(&s.as_str())
}
fn hash_file(path: &str) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    let hash = hasher.finalize();

    Ok(Base64::encode_string(&hash))
}
fn hash_symlink(path: &str) -> io::Result<String> {
    let p_path = fs::read_link(path)?;
    let outpath = p_path.to_str().ok_or(std::io::Error::new(
        std::io::ErrorKind::Other,
        "invalid Unicode",
    ))?;

    Ok(hash_string(&outpath))
}

pub fn hash_tree(path: &str) -> io::Result<HashTreeNode> {
    let p_path = Path::new(path);
    let file_name = p_path
        .file_name()
        .ok_or(std::io::Error::new(
            std::io::ErrorKind::Other,
            "invalid file_name",
        ))?
        .to_str()
        .ok_or(std::io::Error::new(
            std::io::ErrorKind::Other,
            "invalid Unicode",
        ))?;

    let hash;
    let nodetype: Type;
    let mut children: Vec<HashTreeNode> = Vec::new();

    if p_path.is_dir() {
        nodetype = Type::Dir;
        for entry in fs::read_dir(p_path)? {
            let entry_path = entry?.path();
            let path_str = entry_path.to_str().ok_or(std::io::Error::new(
                std::io::ErrorKind::Other,
                "invalid Unicode",
            ))?;
            children.push(hash_tree(&path_str)?);
        }
        children.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        hash = hash_children(&children);
    } else if p_path.is_symlink() {
        nodetype = Type::Symlink;
        hash = hash_symlink(path)?;
    } else {
        nodetype = Type::File;
        hash = hash_file(path)?;
    }

    Ok(HashTreeNode {
        file_name: String::from(file_name),
        nodetype,
        hash,
        children,
    })
}

fn join_paths(root: &String, subdir: &String) -> Option<String> {
    Some(String::from(
        Path::new(&root).join(Path::new(&subdir)).to_str()?,
    ))
}
fn add_prefix(root: &String, v: &Vec<(Type, String)>) -> Option<Vec<(Type, String)>> {
    let mut output: Vec<(Type, String)> = Vec::new();
    for node in v {
        output.push((node.0, join_paths(&root, &node.1)?));
    }
    Some(output)
}
pub fn list_nodes(arg: &HashTreeNode, mode: &Traverse) -> Option<Vec<(Type, String)>> {
    let mut output: Vec<(Type, String)> = Vec::new();
    if mode == &Traverse::Pre {
        output.push((arg.nodetype, arg.file_name.clone()));
    }
    for child in &arg.children {
        for node in list_nodes(child, mode)? {
            output.push((node.0, join_paths(&arg.file_name, &node.1)?));
        }
    }
    if mode == &Traverse::Post {
        output.push((arg.nodetype, arg.file_name.clone()));
    }
    Some(output)
}
fn find<'a>(s: &String, arg: &'a Vec<HashTreeNode>) -> Option<&'a HashTreeNode> {
    match arg.binary_search_by(|ht| ht.file_name.cmp(s)) {
        Ok(val) => Some(&arg[val]),
        Err(_) => None,
    }
}
fn get_children(arg: &HashTreeNode) -> Vec<String> {
    let mut output: Vec<String> = Vec::new();
    for child in &arg.children {
        output.push(child.file_name.clone());
    }
    output
}
fn join_sets(arg0: &Vec<String>, arg1: &Vec<String>) -> Vec<String> {
    let mut i = 0;
    let mut j = 0;

    let mut output: Vec<String> = Vec::new();
    while i < arg0.len() && j < arg1.len() {
        let ord = arg0[i].cmp(&arg1[j]);
        if ord == std::cmp::Ordering::Less {
            output.push(arg0[i].clone());
            i += 1;
        } else if ord == std::cmp::Ordering::Greater {
            output.push(arg1[j].clone());
            j += 1;
        } else {
            output.push(arg0[i].clone());
            i += 1;
            j += 1;
        }
    }

    while i < arg0.len() {
        output.push(arg0[i].clone());
        i += 1;
    }

    while j < arg1.len() {
        output.push(arg1[j].clone());
        j += 1;
    }

    output
}
pub fn diff(old_tree: &HashTreeNode, new_tree: &HashTreeNode) -> Option<Diff> {
    let mut output = Diff {
        added: Vec::new(),
        edited: Vec::new(),
        removed: Vec::new(),
    };
    if old_tree.hash.ne(&new_tree.hash) {
        match (old_tree.nodetype, new_tree.nodetype) {
            (Type::Dir, Type::Dir) => {
                for child in join_sets(&get_children(&old_tree), &get_children(&new_tree)) {
                    let c0 = find(&child, &old_tree.children);
                    let c1 = find(&child, &new_tree.children);

                    match (c0, c1) {
                        (Some(child0), Some(child1)) => {
							let child_output = diff(&child0, &child1)?;
							output.added.append(&mut add_prefix(&old_tree
								.file_name, &child_output.added)?);
							output.edited.append(&mut add_prefix(&old_tree
								.file_name, &child_output.edited)?);
							output.removed.append(&mut add_prefix(&old_tree
								.file_name, &child_output.removed)?);
						}
                        (Some(child0), None) => {
							output.removed.append(&mut add_prefix(&old_tree
								.file_name, &list_nodes(&child0, &Traverse::Post)?)?);
						},
                        (None, Some(child1)) => {
							output.added.append(&mut add_prefix(&new_tree.file_name, &list_nodes(&child1, &Traverse::Pre)?)?);
						},
                        (None, None) => unreachable!("Unexpected error upon set union: an element in the set union does not belong in either of the two original sets"),
                    }
                }
            }
            (type0, type1) if type0 == type1 && type0 != Type::Dir => {
                output
                    .edited
                    .push((old_tree.nodetype, old_tree.file_name.clone()));
            }
            (type0, type1) if type0 != type1 => {
                output
                    .removed
                    .append(&mut list_nodes(&old_tree, &Traverse::Post)?);
                output
                    .added
                    .append(&mut list_nodes(&new_tree, &Traverse::Pre)?);
            }
            _ => unreachable!("The patterns before cover all the possible cases"),
        }
    }
    Some(output)
}

pub fn load_tree(path: &str) -> std::io::Result<HashTreeNode> {
    let serialized = fs::read_to_string(&path)?;
    let ht: HashTreeNode = serde_json::from_str(&serialized)?;
    Ok(ht)
}

pub fn save_tree(path: &str, tree: &HashTreeNode) -> std::io::Result<()> {
    let serialized = serde_json::to_string(&tree)?;
    fs::write(&path, serialized)?;
    Ok(())
}
