#![allow(dead_code)]

use crate::utils;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
    path: PathBuf,
    hash: Option<String>,
}
impl Change {
    pub fn new(action: Action, object: Type, path: PathBuf, hash: Option<String>) -> Change {
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
    pub nodetype: Type,
    pub hash: String,
    pub children: HashMap<PathBuf, HashTreeNode>,
}

fn hash_string(s: &str) -> String {
    let hash = Sha256::digest(s);
    Base64::encode_string(&hash)
}
fn hash_children(children: &HashMap<PathBuf, HashTreeNode>) -> Option<String> {
    let mut s = String::new();
    for (child_name, child_node) in children {
        s += child_name.to_str()?;
        s += child_node.hash.as_str();
    }
    Some(hash_string(&s.as_str()))
}
fn hash_file(path: &PathBuf) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    let hash = hasher.finalize();

    Ok(Base64::encode_string(&hash))
}
fn hash_symlink(path: &PathBuf) -> io::Result<String> {
    let p_path = fs::read_link(path)?;
    let outpath = p_path.to_str().ok_or(std::io::Error::new(
        std::io::ErrorKind::Other,
        "invalid Unicode",
    ))?;

    Ok(hash_string(&outpath))
}

fn to_exclude(path: &PathBuf, exclude_list: &Vec<Regex>) -> std::io::Result<bool> {
    let path_str = match path.to_str() {
        None => {
            return Err(utils::std_err(
                "cannot convert path to str for exclusion check",
            ))
        }
        Some(val) => val,
    };
    Ok(exclude_list.iter().any(|rule| rule.is_match(path_str)))
}
pub fn hash_tree(
    root: &PathBuf,
    rel_path: &PathBuf,
    exclude_list: &Vec<Regex>,
) -> io::Result<HashTreeNode> {
    let full_path = root.join(rel_path);

    let hash;
    let nodetype: Type;
    let mut children: HashMap<PathBuf, HashTreeNode> = HashMap::new();

    if full_path.is_dir() {
        nodetype = Type::Dir;
        for entry in fs::read_dir(&full_path)? {
            let entry_name = {
                let mut entry_path = entry?.path().to_path_buf();

                // Make sure that the directories end with the path separator
                if entry_path.is_dir() {
                    entry_path.push("");
                }

                entry_path
                    .strip_prefix(<PathBuf as AsRef<Path>>::as_ref(&full_path))
                    .map_err(utils::to_io_err)?
                    .to_path_buf()
            };

            let rel_subpath = rel_path.join(&entry_name);
            if !to_exclude(&rel_subpath, exclude_list)? {
                // If for some reasons the whole relative subpath is needed as key
                //	instead of the entry name only, change
                //		entry_name.clone()
                //	with
                //		rel_subpath.clone()
                children.insert(
                    entry_name.clone(),
                    hash_tree(root, &rel_subpath, exclude_list)?,
                );
            }
        }
        hash = hash_children(&children).ok_or(utils::std_err("cannot hash children"))?;
    } else if full_path.is_symlink() {
        nodetype = Type::Symlink;
        hash = hash_symlink(&full_path)?;
    } else {
        nodetype = Type::File;
        hash = hash_file(&full_path)?;
    }

    Ok(HashTreeNode {
        nodetype,
        hash,
        children,
    })
}

fn get_both_keys<T: Clone + Eq + std::hash::Hash, S>(
    arg0: &HashMap<T, S>,
    arg1: &HashMap<T, S>,
) -> Vec<T> {
    let mut output_hm: HashMap<T, bool> = HashMap::new();
    arg0.keys().for_each(|el| {
        output_hm.insert(el.clone(), false);
    });
    arg1.keys().for_each(|el| {
        output_hm.insert(el.clone(), false);
    });
    Vec::from_iter(output_hm.keys().into_iter().map(|el| el.clone()))
}

fn add_prefix_to_changelist(prefix: &PathBuf, vec: &Vec<Change>) -> Vec<Change> {
    let mut output: Vec<Change> = Vec::new();
    for change in vec {
        output.push(Change::new(
            change.action.clone(),
            change.object.clone(),
            prefix.join(&change.path),
            change.hash.clone(),
        ));
    }
    output
}
fn action_on_subtree(arg: &HashTreeNode, action: Action, mode: &Traverse) -> Vec<Change> {
    let mut output: Vec<Change> = Vec::new();
    for (key, child) in &arg.children {
        let hash = match (action, arg.nodetype) {
            (Action::Removed, _) | (_, Type::Dir) => None,
            _ => Some(arg.hash.clone()),
        };
        if mode == &Traverse::Pre {
            output.push(Change::new(
                action.clone(),
                arg.nodetype.clone(),
                key.clone(),
                hash.clone(),
            ));
        }
        output.append(&mut add_prefix_to_changelist(
            &key,
            &action_on_subtree(child, action, mode),
        ));
        if mode == &Traverse::Post {
            output.push(Change::new(
                action.clone(),
                arg.nodetype.clone(),
                key.clone(),
                hash.clone(),
            ));
        }
    }
    output
}

pub fn delta(old_tree: &HashTreeNode, new_tree: &HashTreeNode) -> Vec<Change> {
    let mut output: Vec<Change> = Vec::new();

    for key in get_both_keys(&old_tree.children, &new_tree.children) {
        match (old_tree.children.get(&key), new_tree.children.get(&key)) {
			(Some(child0), None) => {
				let child0_subtree = action_on_subtree(&child0, Action::Removed, &Traverse::Post);
				output.append(&mut add_prefix_to_changelist(&key, &child0_subtree));
			},
			(None, Some(child1)) => {
				let child1_subtree = action_on_subtree(&child1, Action::Added, &Traverse::Pre);
				output.append(&mut add_prefix_to_changelist(&key, &child1_subtree));
			},
			(Some(child0), Some(child1)) => {
				if old_tree.hash.ne(&new_tree.hash) {
					match (child0.nodetype, child1.nodetype) {
						(Type::Dir, Type::Dir) => {
							let children_delta = delta(&child0, &child1);
							output.append(&mut add_prefix_to_changelist(&key, &children_delta));
						},
						(type0, type1) if type0 == type1 => {
							output.push(Change::new(
								Action::Edited,
								type0,
								key.clone(),
								Some(child1.hash.clone())
							));
						},
						(type0, type1) if type0 != type1 => {
							let child0_subtree = action_on_subtree(&child0, Action::Removed, &Traverse::Post);
							output.append(&mut add_prefix_to_changelist(&key, &child0_subtree));
							let child1_subtree = action_on_subtree(&child1, Action::Added, &Traverse::Pre);
							output.append(&mut add_prefix_to_changelist(&key, &child1_subtree));
						},
						_ => unreachable!("The patterns before cover all the possible cases"),
					};
				}
			},
			(None, None) => unreachable!("Unexpected error upon set union: an element in the set union does not belong in either of the two original sets"),
		}
    }
    output
}

pub fn load_tree(path: &PathBuf) -> std::io::Result<HashTreeNode> {
    let serialized = fs::read_to_string(path)?;
    let ht: HashTreeNode = serde_json::from_str(&serialized)?;
    Ok(ht)
}

pub fn save_tree(path: &PathBuf, tree: &HashTreeNode) -> std::io::Result<()> {
    let serialized = serde_json::to_string(&tree)?;
    fs::write(path, serialized)?;
    Ok(())
}
