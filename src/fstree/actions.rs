use std::collections::HashMap;

use crate::{fs::AbstPath, fs::Mtime, hash::Hash};

use super::{Delta, DeltaNode, FSNode, FSTree};

#[allow(clippy::large_enum_variant)]
pub enum ConflictNode {
    Branch(Conflicts),
    Leaf(DeltaNode, DeltaNode),
}
pub struct Conflicts(pub HashMap<String, ConflictNode>);
impl Conflicts {
    pub fn empty() -> Conflicts {
        let conflicts = HashMap::new();
        Conflicts(conflicts)
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    AddDir,
    AddFile(Mtime, Hash),
    AddSymLink(Mtime, Hash),
    EditDir(Mtime),
    EditFile(Option<Mtime>, Option<Hash>),
    EditSymLink(Option<Mtime>, Option<Hash>),
    RemoveDir,
    RemoveFile,
    RemoveSymLink,
}

pub struct Actions(Vec<(AbstPath, Action)>);

#[allow(clippy::new_without_default)]
impl Actions {
    pub fn new() -> Actions {
        Actions(Vec::new())
    }
    pub fn push(&mut self, path: AbstPath, action: Action) {
        let Actions(vec) = self;
        vec.push((path, action));
    }
    pub fn append(&mut self, Actions(appendix): &mut Actions) {
        let Actions(vec) = self;
        vec.append(appendix);
    }
    pub fn add_prefix<S: std::string::ToString>(self, prefix: S) -> Actions {
        let Actions(vec) = self;
        Actions(
            vec.into_iter()
                .map(|(path, action)| (path.add_first(prefix.to_string()), action))
                .collect(),
        )
    }
}
impl IntoIterator for Actions {
    type Item = (AbstPath, Action);

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let Actions(vec) = self;
        vec.into_iter()
    }
}
impl<'a> IntoIterator for &'a Actions {
    type Item = &'a (AbstPath, Action);

    type IntoIter = std::slice::Iter<'a, (AbstPath, Action)>;

    fn into_iter(self) -> Self::IntoIter {
        let Actions(vec) = self;
        vec.iter()
    }
}

pub enum Query {
    File,
    SymLink,
}
pub struct Queries(Vec<(AbstPath, Query)>);

#[allow(clippy::new_without_default)]
impl Queries {
    pub fn new() -> Self {
        Queries(Vec::new())
    }
    pub fn push(&mut self, path: AbstPath, query: Query) {
        let Queries(vec) = self;
        vec.push((path, query));
    }
}

impl FSNode {
    fn to_add_actions(&self) -> Actions {
        let mut actions = Actions::new();
        match self {
            FSNode::File(mtime, hash) => actions.push(
                AbstPath::empty(),
                Action::AddFile(mtime.clone(), hash.clone()),
            ),
            FSNode::SymLink(mtime, hash) => actions.push(
                AbstPath::empty(),
                Action::AddSymLink(mtime.clone(), hash.clone()),
            ),
            FSNode::Dir(mtime, _, subtree) => {
                actions.append(&mut subtree.to_add_actions(mtime));
            }
        }
        actions
    }
}
impl FSTree {
    fn to_add_actions(&self, root_mtime: &Mtime) -> Actions {
        let FSTree(tree) = self;
        let mut actions = Actions::new();
        actions.push(AbstPath::empty(), Action::AddDir);
        for (name, child) in tree {
            let mut child_actions = child.to_add_actions().add_prefix(name);
            actions.append(&mut child_actions);
        }
        actions.push(AbstPath::empty(), Action::EditDir(root_mtime.clone()));
        actions
    }
}

impl Delta {
    /// Convert a delta into a series of actions to be performed on the file
    /// system in order to actually apply the delta on the file system and not
    /// just virtually on the fstree
    pub fn to_actions(&self) -> Actions {
        let Delta(delta) = self;
        let mut actions = Actions::new();
        for (name, child) in delta {
            let mut child_actions = child.to_actions().add_prefix(name);
            actions.append(&mut child_actions);
        }
        actions
    }
}
impl DeltaNode {
    fn to_actions(&self) -> Actions {
        let mut actions = Actions::new();
        match self {
            DeltaNode::Branch(optm, subdelta) => {
                actions.append(&mut subdelta.to_actions());
                if let Some((_, postmtime)) = optm {
                    actions.push(AbstPath::empty(), Action::EditDir(postmtime.clone()));
                }
            }
            DeltaNode::Leaf(Some(FSNode::Dir(_, _, _)), Some(FSNode::Dir(_, _, _)))
            | DeltaNode::Leaf(None, None) => {
                // TODO maybe make these errors better?
                panic!("trying to flat an unshaken delta");
            }
            DeltaNode::Leaf(Some(FSNode::File(m0, h0)), Some(FSNode::File(m1, h1))) => {
                let optm = if m0.ne(m1) { Some(m1.clone()) } else { None };
                let opth = if h0.ne(h1) { Some(h1.clone()) } else { None };
                if optm.is_some() || opth.is_some() {
                    actions.push(AbstPath::empty(), Action::EditFile(optm, opth));
                } else {
                    // TODO maybe make these errors better?
                    panic!("trying to flat an unshaken delta");
                }
            }
            DeltaNode::Leaf(Some(FSNode::SymLink(m0, h0)), Some(FSNode::SymLink(m1, h1))) => {
                let optm = if m0.ne(m1) { Some(m1.clone()) } else { None };
                let opth = if h0.ne(h1) { Some(h1.clone()) } else { None };
                if optm.is_some() || opth.is_some() {
                    actions.push(AbstPath::empty(), Action::EditSymLink(optm, opth));
                } else {
                    // TODO maybe make these errors better?
                    panic!("trying to flat an unshaken delta");
                }
            }
            DeltaNode::Leaf(pre, post) => {
                match pre {
                    Some(FSNode::File(_, _)) => actions.push(AbstPath::empty(), Action::RemoveFile),
                    Some(FSNode::SymLink(_, _)) => {
                        actions.push(AbstPath::empty(), Action::RemoveSymLink)
                    }
                    Some(FSNode::Dir(_, _, _)) => {
                        // NOTE: because the action performed on Action::RemoveDir is
                        //	fs::remove_dir_all and not just fs::remove_dir, there is no
                        //	reason to add actions to remove the content of the directory
                        //	before adding the action of removing the directory
                        actions.push(AbstPath::empty(), Action::RemoveDir);
                    }
                    None => {}
                }
                match post {
                    Some(FSNode::File(mtime, hash)) => {
                        actions.push(
                            AbstPath::empty(),
                            Action::AddFile(mtime.clone(), hash.clone()),
                        );
                    }
                    Some(FSNode::SymLink(mtime, hash)) => {
                        actions.push(
                            AbstPath::empty(),
                            Action::AddSymLink(mtime.clone(), hash.clone()),
                        );
                    }
                    Some(FSNode::Dir(mtime, _, subtree)) => {
                        actions.append(&mut subtree.to_add_actions(mtime));
                    }
                    None => {}
                }
            }
        }

        actions
    }
}

fn add_tree_actions_or_conflicts(
    FSTree(loc_tree): &FSTree,
    FSTree(miss_tree): &FSTree,
) -> Result<Actions, ()> {
    let mut necessary_actions = Actions::new();
    for (name, miss_child) in miss_tree {
        match (loc_tree.get(name), miss_child) {
            (None, _) => {
                let mut add_child_actions = miss_child.to_add_actions().add_prefix(name);
                necessary_actions.append(&mut add_child_actions);
            }
            (Some(FSNode::File(loc_mtime, loc_hash)), FSNode::File(miss_mtime, miss_hash))
                if miss_hash == loc_hash =>
            {
                if miss_mtime != loc_mtime {
                    necessary_actions.push(
                        AbstPath::single(name),
                        Action::EditFile(Some(miss_mtime.clone()), None),
                    );
                }
            }
            (
                Some(FSNode::SymLink(loc_mtime, loc_hash)),
                FSNode::SymLink(miss_mtime, miss_hash),
            ) if miss_hash == loc_hash => {
                if miss_mtime != loc_mtime {
                    necessary_actions.push(
                        AbstPath::single(name),
                        Action::EditSymLink(Some(miss_mtime.clone()), None),
                    );
                }
            }
            (Some(FSNode::Dir(_, _, loc_subtree)), FSNode::Dir(miss_mtime, _, miss_subtree)) => {
                let subadd = add_tree_actions_or_conflicts(loc_subtree, miss_subtree);
                match subadd {
                    Ok(subactions) => {
                        necessary_actions.append(&mut subactions.add_prefix(name));
                        necessary_actions
                            .push(AbstPath::single(name), Action::EditDir(miss_mtime.clone()));
                    }
                    Err(()) => return Err(()),
                }
            }
            _ => return Err(()),
        }
    }
    Ok(necessary_actions)
}
/// Calculates the necessary updates for a missed delta, given the local delta.
///
/// This function has the only purpuse to resolve only the following situation:
/// there is a `local_delta` (that does `old_fstree -> new_local_fstree`) and
/// there is a `missed_delta` recieved from the server that happened since the
/// last known commit, so the `missed_delta` does
/// `old_fstree -> new_missed_fstree`. The idea is that we want to combine the
/// `local_delta` and the `missed_delta` in such a way to obtain deltas that do
/// `old_tree -> new_missed_fstree -> new_local_fstree` when possibile.
///
/// This is not always possible, and it's possible only when the two deltas are
/// compatible (ie: there are no conflicts, and what is a conflict is explained
/// later). Even if the two deltas are compatible, it's not possible to just
/// apply the `missed_delta` and then calculate the new `local_delta`, because
/// while the `missed_delta` operates on the `old_tree`, the file system
/// operates on the fstree that actually exists in the backed source, which is
/// the `new_local_fstree`, and naively applying the `missed_delta` will apply
/// the changes that are supposed for the `old_tree` on the `new_local_fstree`,
/// which might not be possible. For example, maybe both the `missed_delta` and
/// the `local_delta` deleted the same file, so that file currently doesn't
/// exist on the file system (because it doesn't exist in the
/// `new_local_fstree`) and trying to apply the `missed_delta` will try to
/// delete a file that doesn't exist anymore.
///
/// The goal is to reduce the `missed_delta` to some `necessary_actions` that
/// apply only the necessary changes from the `missed_delta` (in the previous
/// example, the deletion of the already deleted file is not necessary) in order
/// to sync the fstree that lives on the file system with the `missed_delta`,
/// without overriding the changes made in the `local_delta` (which is possible
/// only if the `local_delta` and the `missed_delta` are compatible).
///
/// Two deltas are compatible if they don't overlap or, if they overlap (ie:
/// apply a change on the same object), the way they act on the overlap is
/// "the same", where "acting the same" only regards the existance and content
/// of an object (and, for example, not the mtime). So if one deletes a file and
/// the other edits it, this is not compatible, but if they both delete a file
/// or they both edit it with the SAME final content, these are examples of
/// compatible deltas. Note that this may change in the future, if objects will
/// have more metadata than just the mtime.
///
/// If the two deltas edit or create a file with the same final content, the
/// file will end up having the mtime given in the `missed_delta`, so that no
/// further change is needed for that object in the commit
///
/// This function assumes that both deltas have the same assumptions on the
/// previous state, ie: if they both have a node (`loc_pre -> loc_post`) &
/// (`miss_pre -> miss_post`), it does not check if `loc_pre == miss_pre`, as
/// this check will be done later when trying to apply the `necessary_delta`.
///
/// This function assumes to be working on shaken deltas and will not work as
/// expected otherwise. It does not check if the deltas are shaken for sake of
/// efficency
///
/// This function returns `Ok(necessary_actions)` if there is no conflict,
/// otherwise `Err(conflicts)`
pub fn get_actions_or_conflicts(
    Delta(local): &Delta,
    Delta(missed): &Delta,
) -> Result<Actions, Conflicts> {
    let mut necessary_actions = Actions::new();
    let mut conflicts: HashMap<String, ConflictNode> = HashMap::new();
    for (name, miss_node) in missed {
        match local.get(name) {
            // If this node of the missed update is not present in the local
            //	update, it is a necessary change to be pulled in order to apply
            //	the missed update
            None => {
                let mut miss_node_actions = miss_node.to_actions().add_prefix(name);
                necessary_actions.append(&mut miss_node_actions);
            }

            // If this node of the missed update is present in the local update,
            //	check whether they are compatible or if it is a conflict
            Some(loc_node) => match (loc_node, miss_node) {
                (
                    DeltaNode::Branch(_, loc_subdelta),
                    DeltaNode::Branch(miss_optm, miss_subdelta),
                ) => {
                    match get_actions_or_conflicts(loc_subdelta, miss_subdelta) {
                        Ok(subnecessary) => {
                            necessary_actions.append(&mut subnecessary.add_prefix(name));
                            if let Some((_, miss_postmtime)) = miss_optm {
                                necessary_actions.push(
                                    AbstPath::single(name),
                                    Action::EditDir(miss_postmtime.clone()),
                                );
                            }
                        }
                        Err(subconflicts) => {
                            conflicts.insert(name.clone(), ConflictNode::Branch(subconflicts));
                        }
                    };
                }
                // If the object has been removed by both deltas, this is
                //	compatible behaviour and no further action is needed
                (DeltaNode::Leaf(_, None), DeltaNode::Leaf(_, None)) => {}

                // If the objects have the same content (same hash), the only
                //	edit needed is if the local mtime is different to the missed
                //	mtime, in which case the local mtime is set to the missed
                //	mtime
                (
                    DeltaNode::Leaf(_, Some(FSNode::File(loc_mtime, loc_hash))),
                    DeltaNode::Leaf(_, Some(FSNode::File(miss_mtime, miss_hash))),
                ) if loc_hash == miss_hash => {
                    if loc_mtime != miss_mtime {
                        necessary_actions.push(
                            AbstPath::single(name),
                            Action::EditFile(Some(miss_mtime.clone()), None),
                        );
                    }
                }

                // Symlinks recieve the same treatment as files
                (
                    DeltaNode::Leaf(_, Some(FSNode::SymLink(loc_mtime, loc_hash))),
                    DeltaNode::Leaf(_, Some(FSNode::SymLink(miss_mtime, miss_hash))),
                ) if loc_hash == miss_hash => {
                    if loc_mtime != miss_mtime {
                        necessary_actions.push(
                            AbstPath::single(name),
                            Action::EditSymLink(Some(miss_mtime.clone()), None),
                        );
                    }
                }

                //
                (
                    DeltaNode::Leaf(_, Some(FSNode::Dir(_, _, loc_subtree))),
                    DeltaNode::Leaf(_, Some(FSNode::Dir(miss_mtime, _, miss_subtree))),
                ) => {
                    let subget = add_tree_actions_or_conflicts(loc_subtree, miss_subtree);
                    match subget {
                        Ok(subactions) => {
                            necessary_actions.append(&mut subactions.add_prefix(name));
                            necessary_actions
                                .push(AbstPath::single(name), Action::EditDir(miss_mtime.clone()));
                        }
                        Err(()) => {
                            conflicts.insert(
                                name.clone(),
                                ConflictNode::Leaf(loc_node.clone(), miss_node.clone()),
                            );
                        }
                    }
                }
                _ => {
                    conflicts.insert(
                        name.clone(),
                        ConflictNode::Leaf(loc_node.clone(), miss_node.clone()),
                    );
                }
            },
        }
    }
    if conflicts.is_empty() {
        Ok(necessary_actions)
    } else {
        Err(Conflicts(conflicts))
    }
}
