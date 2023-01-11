use std::collections::HashMap;

use abst_fs::AbstPath;

use super::{Delta, DeltaNode, FSNode, FSTree};

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
#[error(
    "File System Tree Delta Error: unable to rebase delta at the given endpoint on the given tree.\nConflict at path: {0}\nError: {1}"
)]
pub struct UnrebasableDelta(AbstPath, String);
fn unrebaseerr(path: AbstPath, err: impl ToString) -> UnrebasableDelta {
    UnrebasableDelta(path, err.to_string())
}

impl Delta {
    pub fn rebase_from_tree_at_endpoint(
        &self,
        fstree: &FSTree,
        endpoint: &AbstPath,
    ) -> Result<Delta, UnrebasableDelta> {
        // Recursive inner method with initialized parameters with default
        // values
        fn recursion(
            delta: &Delta,
            FSTree(fstree): &FSTree,
            endpoint: &AbstPath,
            relative_path: AbstPath,
        ) -> Result<Delta, UnrebasableDelta> {
            match endpoint.get(0) {
                None => Ok(delta.clone()),
                Some(component) => match fstree.get(component) {
                    Some(FSNode::Dir(mtime, _, subtree)) => {
                        let output = HashMap::from([(
                            component.clone(),
                            DeltaNode::Branch(
                                (mtime.clone(), mtime.clone()),
                                recursion(
                                    delta,
                                    subtree,
                                    &endpoint.strip_first(),
                                    relative_path.add_last(component.clone()),
                                )?,
                            ),
                        )]);
                        Ok(Delta(output))
                    }
                    Some(_) => Err(unrebaseerr(relative_path, "node at path is not directory")),
                    None => Err(unrebaseerr(relative_path, "node at path does not exist")),
                },
            }
        }

        recursion(self, fstree, endpoint, AbstPath::empty())
    }
}
