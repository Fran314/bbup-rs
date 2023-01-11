use super::{delta::UnmergeableDelta, Delta};

use abst_fs::AbstPath;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize, Debug)]
pub struct Commit {
    pub commit_id: String,
    // pub endpoint: AbstPath,
    pub delta: Delta,
}
impl Commit {
    const ID_LEN: usize = 64;
    pub fn base_commit() -> Commit {
        Commit {
            commit_id: Commit::gen_null_id(),
            // endpoint: AbstPath::empty(),
            delta: Delta::empty(),
        }
    }
    pub fn gen_null_id() -> String {
        String::from("0").repeat(Commit::ID_LEN)
    }
    fn gen_rand_id() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"0123456789abcdef";
        let mut rng = rand::thread_rng();

        (0..Commit::ID_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
    pub fn gen_valid_id() -> String {
        let null_id = Commit::gen_null_id();
        let mut id = Commit::gen_rand_id();
        while id == null_id {
            id = Commit::gen_rand_id();
        }
        id
    }
}

#[derive(Error, Debug)]
#[error("Get Update Delta Error: Failed to get the update delta since the last known commit\nproblematic commit id: {0}\nreason: {1}")]
pub struct GetUpdError(String, UnmergeableDelta);

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitList(Vec<Commit>);
impl CommitList {
    pub fn base_commit_list() -> CommitList {
        CommitList(vec![Commit::base_commit()])
    }

    pub fn most_recent_commit(&self) -> &Commit {
        let CommitList(list) = self;
        // TODO unwrap here eeeeeeeeeee
        list.get(list.len() - 1).unwrap()
    }

    pub fn push(&mut self, commit: Commit) {
        let CommitList(list) = self;
        list.push(commit);
    }

    pub fn get_update_delta(&self, endpoint: &AbstPath, lkc: String) -> Result<Delta, GetUpdError> {
        let mut output: Delta = Delta::empty();
        let CommitList(list) = self;
        for commit in list.iter().rev() {
            if commit.commit_id.eq(&lkc) {
                break;
            }
            // let mut delta = commit.delta.clone();
            // let mut commit_endpoint = commit.endpoint.clone();
            // let mut curr_endpoint = endpoint.clone();

            if let Some(delta_at_endpoint) = commit.delta.get_subdelta_tree_copy(&endpoint) {
                if let Err(err) = output.merge_prec(&delta_at_endpoint) {
                    return Err(GetUpdError(commit.commit_id.clone(), err));
                }
            }

            // for component in endpoint {
            //     match commit_endpoint.get(0) {
            //         Some(comp) if component == comp => {
            //             commit_endpoint = commit_endpoint.strip_first();
            //             curr_endpoint = curr_endpoint.strip_first();
            //         }
            //         Some(_) => continue 'commit_loop,
            //         None => break,
            //     }
            // }
            // for component in commit_endpoint.into_iter().rev() {
            //     let node = DeltaNode::Branch(None, delta);
            //     let tree = HashMap::from([(component, node)]);
            //     delta = Delta(tree)
            // }
        }
        Ok(output)
    }
}
