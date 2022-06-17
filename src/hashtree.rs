#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::Path;

use base64ct::{Base64, Encoding};
use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};

use regex::Regex;

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
#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone)]
pub enum Action {
    Added,
    Edited,
    Removed,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Change {
    action: Action,
    object: Type,
    path: String,
    hash: Option<String>,
}
impl Change {
    pub fn new(action: Action, object: Type, path: String, hash: Option<String>) -> Change {
        Change {
            action,
            object,
            path,
            hash,
        }
    }
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

pub fn hash_tree(path: &str, exclude_list: &Vec<Regex>) -> io::Result<HashTreeNode> {
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
            let exclude = exclude_list.iter().any(|rule| rule.is_match(path_str));
            if !exclude {
                children.push(hash_tree(path_str, exclude_list)?);
            }
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

fn join_paths(prefix: &String, suffix: &String) -> Option<String> {
    Some(String::from(
        Path::new(prefix).join(Path::new(suffix)).to_str()?,
    ))
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

fn add_prefix(prefix: &String, vec: &Vec<Change>) -> Option<Vec<Change>> {
    let mut output: Vec<Change> = Vec::new();
    for change in vec {
        output.push(Change::new(
            change.action.clone(),
            change.object.clone(),
            join_paths(prefix, &change.path)?,
            change.hash.clone(),
        ));
    }
    Some(output)
}
fn action_on_subtree(arg: &HashTreeNode, action: Action, mode: &Traverse) -> Option<Vec<Change>> {
    let mut output: Vec<Change> = Vec::new();
    if mode == &Traverse::Pre {
        let hash = match (action, arg.nodetype) {
            (Action::Removed, _) | (_, Type::Dir) => None,
            _ => Some(arg.hash.clone()),
        };
        output.push(Change::new(
            action.clone(),
            arg.nodetype.clone(),
            arg.file_name.clone(),
            hash,
        ));
    }
    for child in &arg.children {
        let child_subtree = action_on_subtree(child, action.clone(), mode)?;
        output.append(&mut add_prefix(&arg.file_name, &child_subtree)?);
    }
    if mode == &Traverse::Post {
        let hash = match (action, arg.nodetype) {
            (Action::Removed, _) | (_, Type::Dir) => None,
            _ => Some(arg.hash.clone()),
        };
        output.push(Change::new(
            action.clone(),
            arg.nodetype.clone(),
            arg.file_name.clone(),
            hash,
        ));
    }
    Some(output)
}
pub fn delta(old_tree: &HashTreeNode, new_tree: &HashTreeNode) -> Option<Vec<Change>> {
    let mut output: Vec<Change> = Vec::new();
    if old_tree.hash.ne(&new_tree.hash) {
        match (old_tree.nodetype, new_tree.nodetype) {
            (Type::Dir, Type::Dir) => {
                let dir_name = &old_tree.file_name; // == &old_tree.file_name, proof by recursion
                for child in join_sets(&get_children(&old_tree), &get_children(&new_tree)) {
                    let c0 = find(&child, &old_tree.children);
                    let c1 = find(&child, &new_tree.children);

                    match (c0, c1) {
                        (Some(child0), Some(child1)) => {
							let children_delta = delta(&child0, &child1)?;
							output.append(&mut add_prefix(dir_name, &children_delta)?);
						}
                        (Some(child0), None) => {
							let child0_subtree = action_on_subtree(&child0, Action::Removed, &Traverse::Post)?;
							output.append(&mut add_prefix(dir_name, &child0_subtree)?);
						},
                        (None, Some(child1)) => {
							let child1_subtree = action_on_subtree(&child1, Action::Added, &Traverse::Pre)?;
							output.append(&mut add_prefix(dir_name, &child1_subtree)?);
						},
                        (None, None) => unreachable!("Unexpected error upon set union: an element in the set union does not belong in either of the two original sets"),
                    }
                }
            }
            (type0, type1) if type0 == type1 && type0 != Type::Dir => {
                output.push(Change::new(
                    Action::Edited,
                    new_tree.nodetype.clone(),
                    new_tree.file_name.clone(),
                    Some(new_tree.hash.clone()),
                ));
            }
            (type0, type1) if type0 != type1 => {
                output.append(&mut action_on_subtree(
                    &old_tree,
                    Action::Removed,
                    &Traverse::Post,
                )?);
                output.append(&mut action_on_subtree(
                    &new_tree,
                    Action::Added,
                    &Traverse::Pre,
                )?);
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
