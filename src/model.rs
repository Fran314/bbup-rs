use crate::path::{AbstractPath, FileType};

use serde::{Deserialize, Serialize};

use regex::Regex;

use thiserror::Error;

//--- STUFF TO SORT ---//
#[derive(Serialize, Deserialize, Debug, Clone)]
/// Enumerate the types of addition that can be done
pub enum Adding {
    Dir,

    /// `FileType(FileType::File | FileType::SymLink, hash)` where `hash` is the hash of the content of the file added
    FileType(FileType, String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Enumerate the types of edit that can be done
pub enum Editing {
    /// `FileType(FileType::File | FileType::SymLink, hash)` where `hash` is the hash of the content of the file added
    FileType(FileType, String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Enumerate the types of removal that can be done
pub enum Removing {
    Dir,
    FileType(FileType),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Wrapper containint the type of the change done
pub enum ChangeType {
    Added(Adding),
    Edited(Editing),
    Removed(Removing),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Struct containing all the necessary information
/// on a change that occurred between hashtrees
pub struct Change {
    /// Path where the change occurred
    pub path: AbstractPath,

    /// Type of change that occurred
    pub change_type: ChangeType,
}

pub type Delta = Vec<Change>;
pub trait DeltaExt {
    fn merge_delta(&mut self, prec: &Delta);
}
impl DeltaExt for Delta {
    fn merge_delta(&mut self, prec: &Delta) {
        for prec_change in prec {
            match self
                .into_iter()
                .position(|change| change.path.eq(&prec_change.path))
            {
                None => self.push(prec_change.clone()),
                Some(pos) => {
                    let succ_change = self[pos].clone();
                    match (
                        prec_change.change_type.clone(),
                        succ_change.change_type.clone(),
                    ) {
                        (ChangeType::Added(_), ChangeType::Added(_))
                        | (ChangeType::Edited(_), ChangeType::Added(_))
                        | (ChangeType::Removed(_), ChangeType::Edited(_))
                        | (ChangeType::Removed(_), ChangeType::Removed(_)) => {
                            panic!("Commit list is broken! Succession of incompatible changes is saved in the commit list\nat path: {:?}\nchange {:?} occurred after previous change {:?}, and these are incompatible", prec_change.path, succ_change.change_type, prec_change.change_type)
                        }

                        // If object is added and later on edited, it's the same as adding it with the new content (hash1)
                        (ChangeType::Added(add0), ChangeType::Edited(edit1)) => {
                            let add = match (add0, edit1) {
								(
									Adding::FileType(type0, _),
									Editing::FileType(type1, hash1),
								) if type0 == type1 => Adding::FileType(type1, hash1),
								_ => panic!("Commit list is broken! Succession of incompatible changes is saved in the commit list\nentry type mismatch for path: {:?}", succ_change.path),
							};
                            self[pos] = Change {
                                path: succ_change.path.clone(),
                                change_type: ChangeType::Added(add),
                            }
                        }

                        // If object is added and later on removed, it's the same as not mentioning it at all
                        (ChangeType::Added(_), ChangeType::Removed(_)) => {
                            self.remove(pos);
                        }

                        // If object is edited twice, it's the same as editing it once with the new content (succ hash)
                        // That said, because a double edit results in an edit containing the most recent hash value,
                        //	and main[pos] is already the an edit containing the most recent hash value, merging these
                        //	two changes means doing absolutely nothing, hence why we're doing nothing in this branch
                        // Basically the same happens when a removal happens after an edit. An edit succeded by a
                        //	removal results in only a removal, and main[pos] is already such a removal
                        (ChangeType::Edited(_), ChangeType::Edited(_)) => { /* Do nothing */ }
                        (ChangeType::Edited(_), ChangeType::Removed(_)) => { /* Do nothing */ }

                        // If object is removed and later on added, we have three cases:
                        //	- (A) The entry types of the removed entry and the added entry match, and they're both a dir
                        //		In this case, it's the same as just doing nothing at all
                        //	- (B) The entry types match, and they're something else (file or symlink)
                        //		In this case, it's the same as editing the object with the hash derived from the addition
                        //	- (C) The entry types do not match
                        //		In this case we just have both the removal of the old object and the
                        //		addition of the new object. Because the addition is already in main, we only have to add
                        //		insert the removal in main
                        (ChangeType::Removed(remove0), ChangeType::Added(add0)) => {
                            match (remove0, add0) {
                                // Case (A)
                                (Removing::Dir, Adding::Dir) => { /* Do nothing */ }

                                // Case (B)
                                (Removing::FileType(type0), Adding::FileType(type1, hash1))
                                    if type0 == type1 =>
                                {
                                    let edit = Editing::FileType(type1, hash1);
                                    self[pos] = Change {
                                        path: succ_change.path.clone(),
                                        change_type: ChangeType::Edited(edit),
                                    }
                                }

                                // Case (C)
                                _ => {
                                    self.push(prec_change.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
pub trait PrettyPrint {
    fn pretty_print(&self, indent: u8) -> String;
}
impl PrettyPrint for Delta {
    fn pretty_print(&self, indent: u8) -> String {
        let ind = String::from("\t".repeat(indent as usize));
        let mut output = String::new();
        for i in 0..self.len() {
            output += ind.as_str();

            output += match self[i].change_type {
                ChangeType::Added(Adding::Dir) => "+++  dir   ",
                ChangeType::Added(Adding::FileType(FileType::File, _)) => "+++  file  ",
                ChangeType::Added(Adding::FileType(FileType::SymLink, _)) => "+++  sylk  ",
                ChangeType::Edited(Editing::FileType(FileType::File, _)) => "~~~  file  ",
                ChangeType::Edited(Editing::FileType(FileType::SymLink, _)) => "~~~  sylk  ",
                ChangeType::Removed(Removing::Dir) => "---  dir   ",
                ChangeType::Removed(Removing::FileType(FileType::File)) => "---  file  ",
                ChangeType::Removed(Removing::FileType(FileType::SymLink)) => "---  sylk  ",
            };
            output += self[i].path.to_string().as_str();
            if i != self.len() - 1 {
                output += "\n";
            }
        }
        output
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Commit {
    pub commit_id: String,
    pub delta: Delta,
}
impl Commit {
    pub fn base_commit() -> Commit {
        Commit {
            commit_id: String::from("0").repeat(64),
            delta: Vec::new(),
        }
    }
}

pub struct ExcludeList {
    list: Vec<Regex>,
}

#[derive(Error, Debug)]
pub enum ExcludeListError {
    #[error("Failed to parse rule to regex\nrule: {rule}\nreason: {error:?}")]
    UnparsableRule { rule: String, error: regex::Error },
}
impl ExcludeList {
    pub fn from(rules: &Vec<String>) -> Result<ExcludeList, ExcludeListError> {
        ExcludeList {
            list: vec![Regex::new("\\.bbup/").unwrap()],
        }
        .join(rules)
    }
    pub fn join(&self, rules: &Vec<String>) -> Result<ExcludeList, ExcludeListError> {
        let mut list = self.list.clone();
        for rule in rules {
            let rgx = match Regex::new(rule) {
                Ok(val) => val,
                Err(error) => {
                    return Err(ExcludeListError::UnparsableRule {
                        rule: rule.to_string(),
                        error,
                    })
                }
            };
            list.push(rgx);
        }
        Ok(ExcludeList { list })
    }
    pub fn should_exclude(&self, path: &AbstractPath, is_dir: bool) -> bool {
        let path_as_string = {
            let mut tmp = path.to_string();
            if is_dir {
                tmp.push(std::path::MAIN_SEPARATOR);
            }
            tmp
        };

        for rule in &self.list {
            if rule.is_match(path_as_string.as_str()) {
                return true;
            }
        }

        false
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum JobType {
    Pull,
    Push,
    Quit,
}
