use crate::utils;

use crate::structs::{Action, Change, Delta, ObjectType};

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use base64ct::{Base64, Encoding};
use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};

use regex::Regex;

#[derive(PartialEq)]
enum Traverse {
    Pre,
    Post,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HashTreeNode {
    pub nodetype: ObjectType,
    pub hash: String,
    pub children: HashMap<PathBuf, HashTreeNode>,
}

fn hash_string(s: &str) -> String {
    let hash = Sha256::digest(s);
    Base64::encode_string(&hash)
}
fn hash_children(children: &HashMap<PathBuf, HashTreeNode>) -> std::io::Result<String> {
    let mut s = String::new();
    for (child_name, child_node) in children {
        s += child_name
            .to_str()
            .ok_or(utils::std_err("cannot convert child name to str"))?;
        s += child_node.hash.as_str();
    }
    Ok(hash_string(&s.as_str()))
}
fn hash_file(path: &PathBuf) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let hash = hasher.finalize();

    Ok(Base64::encode_string(&hash))
}
fn hash_symlink(path: &PathBuf) -> std::io::Result<String> {
    let p_path = fs::read_link(path)?;
    let outpath = p_path.to_str().ok_or(utils::std_err("invalid Unicode"))?;

    Ok(hash_string(&outpath))
}
pub fn get_hash_tree(root: &PathBuf, exclude_list: &Vec<Regex>) -> std::io::Result<HashTreeNode> {
    get_hash_tree_rec(root, &std::path::Path::new("").to_path_buf(), exclude_list)
}
fn get_hash_tree_rec(
    root: &PathBuf,
    rel_path: &PathBuf,
    exclude_list: &Vec<Regex>,
) -> std::io::Result<HashTreeNode> {
    let full_path = root.join(rel_path);

    let hash;
    let nodetype: ObjectType;
    let mut children: HashMap<PathBuf, HashTreeNode> = HashMap::new();

    if full_path.is_dir() {
        nodetype = ObjectType::Dir;
        for entry in fs::read_dir(&full_path)? {
            let entry_name = {
                let entry_path = entry?.path().to_path_buf();
                let mut entry_name = entry_path
                    .strip_prefix(<PathBuf as AsRef<Path>>::as_ref(&full_path))
                    .map_err(utils::to_io_err)?
                    .to_path_buf();

                // Make sure that the directories end with the path separator
                if entry_path.is_dir() {
                    entry_name.push("");
                }
                entry_name
            };

            let rel_subpath = rel_path.join(&entry_name);
            if !utils::to_exclude(&rel_subpath, exclude_list)? {
                // If for some reasons the whole relative subpath is needed as key
                //	instead of the entry name only, change
                //		entry_name.clone()
                //	with
                //		rel_subpath.clone()
                children.insert(
                    entry_name.clone(),
                    get_hash_tree_rec(root, &rel_subpath, exclude_list)?,
                );
            }
        }
        hash = hash_children(&children)?;
    } else if full_path.is_symlink() {
        nodetype = ObjectType::Symlink;
        hash = hash_symlink(&full_path)?;
    } else {
        nodetype = ObjectType::File;
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
fn add_prefix_to_changelist(prefix: &PathBuf, vec: &Delta) -> Delta {
    Vec::from_iter(vec.into_iter().map(|el| Change {
        path: prefix.join(&el.path),
        ..el.clone()
    }))
}
fn action_on_subtree(arg: &HashTreeNode, action: Action, mode: &Traverse) -> Delta {
    let mut output: Delta = Vec::new();
    for (key, child) in &arg.children {
        let hash = match (action, child.nodetype) {
            (Action::Removed, _) | (_, ObjectType::Dir) => None,
            _ => Some(child.hash.clone()),
        };
        let child_change = Change::new(action.clone(), child.nodetype.clone(), key.clone(), hash);
        if mode == &Traverse::Pre {
            output.push(child_change.clone());
        }
        output.append(&mut add_prefix_to_changelist(
            &key,
            &action_on_subtree(child, action, mode),
        ));
        if mode == &Traverse::Post {
            output.push(child_change.clone());
        }
    }
    output
}

pub fn delta(old_tree: &HashTreeNode, new_tree: &HashTreeNode) -> Delta {
    let mut output: Delta = Vec::new();

    for key in get_both_keys(&old_tree.children, &new_tree.children) {
        match (old_tree.children.get(&key), new_tree.children.get(&key)) {
			(Some(child0), None) => {
				let child0_subtree = action_on_subtree(&child0, Action::Removed, &Traverse::Post);
				output.append(&mut add_prefix_to_changelist(&key, &child0_subtree));
				output.push(Change::new(Action::Removed, child0.nodetype.clone(), key.clone(), None));
			},
			(None, Some(child1)) => {
				let child1_subtree = action_on_subtree(&child1, Action::Added, &Traverse::Pre);
				let hash = match child1.nodetype {
					ObjectType::Dir => None,
					_ => Some(child1.hash.clone()),
				};
				output.push(Change::new(Action::Added, child1.nodetype.clone(), key.clone(), hash));
				output.append(&mut add_prefix_to_changelist(&key, &child1_subtree));
			},
			(Some(child0), Some(child1)) => {
				if child0.hash.ne(&child1.hash) {
					match (child0.nodetype, child1.nodetype) {
						(ObjectType::Dir, ObjectType::Dir) => {
							let children_delta = delta(&child0, &child1);
							output.append(&mut add_prefix_to_changelist(&key, &children_delta));
						},
						(type0, type1) if type0 == type1 => {
							output.push(Change::new(
								Action::Edited,
								type1,
								key.clone(),
								Some(child1.hash.clone())
							));
						},
						(type0, type1) if type0 != type1 => {
							let child0_subtree = action_on_subtree(&child0, Action::Removed, &Traverse::Post);
							output.append(&mut add_prefix_to_changelist(&key, &child0_subtree));
							output.push(Change::new(Action::Removed, child0.nodetype.clone(), key.clone(), None));

							let child1_subtree = action_on_subtree(&child1, Action::Added, &Traverse::Pre);
							let hash = match child1.nodetype {
								ObjectType::Dir => None,
								_ => Some(child1.hash.clone()),
							};
							output.push(Change::new(Action::Added, child1.nodetype.clone(), key.clone(), hash));
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
