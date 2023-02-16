use super::{delta::UnmergeableDelta, Delta};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct CommitID([u8; 32]);
impl std::fmt::Display for CommitID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut output = String::new();
        self.0
            .iter()
            .for_each(|byte| output += format!("{byte:02x}").as_str());
        write!(f, "{output:?}")
    }
}
impl CommitID {
    pub fn gen_null() -> CommitID {
        CommitID([0u8; 32])
    }
    pub fn gen_rand() -> CommitID {
        use rand::Rng;
        CommitID(rand::thread_rng().gen())
    }
    pub fn gen_valid() -> CommitID {
        let null_id = CommitID::gen_null();
        let mut id = CommitID::gen_rand();
        while id == null_id {
            id = CommitID::gen_rand();
        }
        id
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Commit {
    pub commit_id: CommitID,
    pub delta: Delta,
}
impl Commit {
    pub fn base_commit() -> Commit {
        Commit {
            commit_id: CommitID::gen_null(),
            delta: Delta::new(),
        }
    }
}

#[derive(Error, Debug)]
pub enum GetUpdError {
    #[error(
        "Get Update Delta Error: the provided commit id does not exist\nprovided commit id: {0}"
    )]
    MissingId(CommitID),

    #[error("Get Update Delta Error: Failed to get the update delta since the last known commit\nproblematic commit id: {0}\nreason: {1}")]
    MergeError(CommitID, UnmergeableDelta),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitList(Vec<Commit>);
impl CommitList {
    pub fn base_commit_list() -> CommitList {
        CommitList(vec![Commit::base_commit()])
    }

    pub fn most_recent_commit(&self) -> &Commit {
        let CommitList(list) = self;
        // TODO
        // I'm not sure that unwrap is the best solution here. It is true that
        // the public API does not allow to create an empty Commit List (it
        // will always have at least the base commit) so this shouldn't be a
        // problem for anyone who uses this library correctly, but it still
        // doesn't feel particularly correct to have unwrap here instead of
        // returning an option
        list.last().unwrap()
    }

    pub fn push(&mut self, commit: Commit) {
        let CommitList(list) = self;
        list.push(commit);
    }
    pub fn get_update_delta(&self, lkc: CommitID) -> Result<Delta, GetUpdError> {
        let mut output: Delta = Delta::new();
        let CommitList(list) = self;
        for commit in list.iter().rev() {
            if commit.commit_id.eq(&lkc) {
                return Ok(output);
            }
            if let Err(err) = output.merge_prec(&commit.delta) {
                return Err(GetUpdError::MergeError(commit.commit_id.clone(), err));
            }
        }
        Err(GetUpdError::MissingId(lkc))
    }
}
