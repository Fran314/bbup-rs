use crate::fs::PathExt;
use crate::fstree::DeltaFSTree;

use serde::{Deserialize, Serialize};

use regex::Regex;

use thiserror::Error;
#[derive(Serialize, Deserialize, Debug)]
pub struct Commit {
    pub commit_id: String,
    pub delta: DeltaFSTree,
}
impl Commit {
    pub fn base_commit() -> Commit {
        Commit {
            commit_id: String::from("0").repeat(64),
            delta: DeltaFSTree::empty(),
        }
    }
}

pub struct ExcludeList {
    list: Vec<Regex>,
}
#[derive(Error, Debug)]
pub enum ExcludeListError {
    #[error("Exclude List Error: Failed to parse rule to regex\nrule: {rule}\nreason: {err:?}")]
    UnparsableRule { rule: String, err: regex::Error },
}
fn unparerr<S: std::string::ToString>(rule: S) -> impl Fn(regex::Error) -> ExcludeListError {
    move |err: regex::Error| -> ExcludeListError {
        ExcludeListError::UnparsableRule {
            rule: rule.to_string(),
            err,
        }
    }
}
impl ExcludeList {
    pub fn from(rules: &Vec<String>) -> Result<ExcludeList, ExcludeListError> {
        ExcludeList {
            list: vec![Regex::new("\\.bbup/").unwrap()],
        }
        .join(rules)
    }
    pub fn join(self, rules: &Vec<String>) -> Result<ExcludeList, ExcludeListError> {
        let mut list = self.list.clone();
        for rule in rules {
            let rgx = Regex::new(rule).map_err(unparerr(rule))?;
            list.push(rgx);
        }
        Ok(ExcludeList { list })
    }

    // TODO this should somehow implement the blob pattern thingy
    pub fn should_exclude<P: AsRef<std::path::Path>>(&self, path: P, is_dir: bool) -> bool {
        let path_as_string = {
            let mut tmp = path.as_ref().force_to_string();
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
