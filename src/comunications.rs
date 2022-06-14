use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Basic {
    pub content: String,
}
impl Basic {
    pub fn new(content: &str) -> Basic {
        Basic {
            content: content.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LastCommit {
    pub commit_id: String,
}
impl LastCommit {
    pub fn new(commit_id: &str) -> LastCommit {
        LastCommit {
            commit_id: commit_id.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Commit {
    pub delta: Vec<(u8, u8, String)>,
}
impl Commit {
    pub fn new(delta: Vec<(u8, u8, String)>) -> Commit {
        Commit { delta }
    }
}
