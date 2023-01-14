use abst_fs::{AbstPath, Mtime};

use hasher::Hash;

use super::{Delta, DeltaNode, FSNode, FSTree};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    AddDir,
    AddFile(Mtime, Hash),
    AddSymLink(Mtime, Hash),
    EditDir(Mtime),
    EditFile(Mtime, Option<Hash>),
    EditSymLink(Mtime, Option<Hash>),
    RemoveDir,
    RemoveFile,
    RemoveSymLink,
}

#[derive(Debug)]
pub struct Actions(Vec<(AbstPath, Action)>);
impl PartialEq for Actions {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self.0.iter().all(|(path, action)| {
                other
                    .0
                    .iter()
                    .any(|(other_path, other_action)| path == other_path && action == other_action)
            })
    }
}

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

#[derive(Error, Debug, PartialEq)]
#[error(
    "File System Tree Delta Error: unable to convert delta to actions.\nError at path: {0}\nError: {1}"
)]
pub struct ToActErr(AbstPath, String);
fn toacterr(path: AbstPath, err: impl ToString) -> ToActErr {
    ToActErr(path, err.to_string())
}
fn push_toacterr(parent: impl ToString) -> impl Fn(ToActErr) -> ToActErr {
    move |ToActErr(path, err)| ToActErr(path.add_first(parent.to_string()), err)
}

#[derive(Error, Debug, PartialEq)]
#[error(
    "File System Tree Delta Error: unable to convert delta to actions.\nError at path: {0}\nError: {1}"
)]
pub struct GetActErr(AbstPath, String);
fn getacterr(path: AbstPath, err: impl ToString) -> GetActErr {
    GetActErr(path, err.to_string())
}
fn push_getacterr(parent: impl ToString) -> impl Fn(GetActErr) -> GetActErr {
    move |GetActErr(path, err)| GetActErr(path.add_first(parent.to_string()), err)
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
    pub fn to_actions(&self) -> Result<Actions, ToActErr> {
        let Delta(delta) = self;
        let mut actions = Actions::new();
        for (name, child) in delta {
            let mut child_actions = child
                .to_actions()
                .map_err(push_toacterr(name))?
                .add_prefix(name);
            actions.append(&mut child_actions);
        }
        Ok(actions)
    }
}
impl DeltaNode {
    fn to_actions(&self) -> Result<Actions, ToActErr> {
        let mut actions = Actions::new();
        match self {
            DeltaNode::Branch((_, postmtime), subdelta) => {
                actions.append(&mut subdelta.to_actions()?);
                actions.push(AbstPath::empty(), Action::EditDir(postmtime.clone()));
            }
            DeltaNode::Leaf(Some(FSNode::Dir(_, _, _)), Some(FSNode::Dir(_, _, _))) => {
                return Err(toacterr(
                    AbstPath::empty(),
                    "delta is not shaken at path, leaf from dir to dir",
                ));
            }
            DeltaNode::Leaf(None, None) => {
                return Err(toacterr(
                    AbstPath::empty(),
                    "delta is not shaken at path, leaf from none to none",
                ));
            }
            DeltaNode::Leaf(Some(FSNode::File(m0, h0)), Some(FSNode::File(m1, h1))) => {
                if m0.ne(m1) || h0.ne(h1) {
                    let opth = if h0.ne(h1) { Some(h1.clone()) } else { None };
                    actions.push(AbstPath::empty(), Action::EditFile(m1.clone(), opth));
                } else {
                    return Err(toacterr(
                        AbstPath::empty(),
                        "delta is not shaken at path, leaf from file to identical file",
                    ));
                }
            }
            DeltaNode::Leaf(Some(FSNode::SymLink(m0, h0)), Some(FSNode::SymLink(m1, h1))) => {
                if m0.ne(m1) || h0.ne(h1) {
                    let opth = if h0.ne(h1) { Some(h1.clone()) } else { None };
                    actions.push(AbstPath::empty(), Action::EditSymLink(m1.clone(), opth));
                } else {
                    return Err(toacterr(
                        AbstPath::empty(),
                        "delta is not shaken at path, leaf from symlink to identical symlink",
                    ));
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

        Ok(actions)
    }
}

/// This function is needed only for one specific branch of the
/// `get_actions` function. More precisely, if both the deltas
/// added a directory, the goal of this function is to add the files in the
/// missed version of the directory to the local version of the directory (if
/// this is possible, ie there are no conflicts on overlapping files defined in
/// different ways). If it's not possible, return a conflict on why it is not
/// possible
fn add_tree_actions(
    FSTree(loc_tree): &FSTree,
    FSTree(miss_tree): &FSTree,
) -> Result<Actions, GetActErr> {
    let mut necessary_actions = Actions::new();
    for (name, miss_child) in miss_tree {
        match (loc_tree.get(name), miss_child) {
            (None, _) => {
                let mut add_child_actions = miss_child.to_add_actions().add_prefix(name);
                necessary_actions.append(&mut add_child_actions);
            }
            (Some(FSNode::File(loc_mtime, loc_hash)), FSNode::File(miss_mtime, miss_hash)) => {
                if miss_hash == loc_hash {
                    if miss_mtime != loc_mtime {
                        necessary_actions.push(
                            AbstPath::single(name),
                            Action::EditFile(miss_mtime.clone(), None),
                        );
                    }
                } else {
                    return Err(getacterr(
                        AbstPath::single(name),
                        "adding incompatible files with different contents",
                    ));
                }
            }
            (
                Some(FSNode::SymLink(loc_mtime, loc_hash)),
                FSNode::SymLink(miss_mtime, miss_hash),
            ) => {
                if miss_hash == loc_hash {
                    if miss_mtime != loc_mtime {
                        necessary_actions.push(
                            AbstPath::single(name),
                            Action::EditSymLink(miss_mtime.clone(), None),
                        );
                    }
                } else {
                    return Err(getacterr(
                        AbstPath::single(name),
                        "adding incompatible symlinks with different endpoints",
                    ));
                }
            }
            (Some(FSNode::Dir(_, _, loc_subtree)), FSNode::Dir(miss_mtime, _, miss_subtree)) => {
                let subadd =
                    add_tree_actions(loc_subtree, miss_subtree).map_err(push_getacterr(name))?;
                necessary_actions.append(&mut subadd.add_prefix(name));

                // IMPORTANT: this edit dir action is necessary even if
                //	loc_mtime == miss_mtime, because the subactions executed
                //	before this will probably change the actual mtime of
                //	the directory on the file system
                necessary_actions.push(AbstPath::single(name), Action::EditDir(miss_mtime.clone()));
            }
            _ => {
                return Err(getacterr(
                    AbstPath::single(name),
                    "adding incompatible objects",
                ))
            }
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
/// `old_tree -> new_missed_fstree -> new_local_fstree` when possible.
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
/// this check will be done later when trying to apply the `missed_delta`.
///
/// This function assumes to be working on shaken deltas and will not work as
/// expected otherwise. It does not check if the deltas are shaken for sake of
/// efficency
///
/// This function returns `Ok(necessary_actions)` if there is no conflict,
/// otherwise `Err(conflicts)`
pub fn get_actions(Delta(local): &Delta, Delta(missed): &Delta) -> Result<Actions, GetActErr> {
    let mut necessary_actions = Actions::new();
    for (name, miss_node) in missed {
        match local.get(name) {
            // If this node of the missed update is not present in the local
            //	update, it is a necessary change to be pulled in order to apply
            //	the missed update
            None => match miss_node.to_actions() {
                Ok(actions) => necessary_actions.append(&mut actions.add_prefix(name)),
                Err(ToActErr(path, err)) => {
                    return Err(getacterr(
                        path.add_first(name),
                        format!("missed delta of locally untouched object is not shaken\n{err}"),
                    ));
                }
            },

            // If this node of the missed update is present in the local update,
            //	check whether they are compatible or if it is a conflict
            Some(loc_node) => match (loc_node, miss_node) {
                // If they both changed the content of the node-directory (and
                //	optionally the mtime), recurse on the content of the
                //	directory and set as final mtime the one from missed delta
                (
                    DeltaNode::Branch(_, loc_subdelta),
                    DeltaNode::Branch((_, miss_postmtime), miss_subdelta),
                ) => {
                    let subactions =
                        get_actions(loc_subdelta, miss_subdelta).map_err(push_getacterr(name))?;
                    necessary_actions.append(&mut subactions.add_prefix(name));
                    necessary_actions.push(
                        AbstPath::single(name),
                        Action::EditDir(miss_postmtime.clone()),
                    );
                }
                // If the object has been removed by both deltas, this is
                //	compatible behaviour and no further action is needed
                (DeltaNode::Leaf(_, None), DeltaNode::Leaf(_, None)) => {}

                (
                    DeltaNode::Leaf(_, Some(FSNode::File(loc_mtime, loc_hash))),
                    DeltaNode::Leaf(_, Some(FSNode::File(miss_mtime, miss_hash))),
                ) => {
                    if loc_hash == miss_hash {
                        // If the objects have the same content (same hash), the only
                        //	edit needed is if the local mtime is different to the missed
                        //	mtime, in which case the local mtime is set to the missed
                        //	mtime
                        if loc_mtime != miss_mtime {
                            necessary_actions.push(
                                AbstPath::single(name),
                                Action::EditFile(miss_mtime.clone(), None),
                            );
                        }
                    } else {
                        return Err(getacterr(
                            AbstPath::single(name),
                            "added or edited incompatible files with different content",
                        ));
                    }
                }

                // Symlinks recieve the same treatment as files
                (
                    DeltaNode::Leaf(_, Some(FSNode::SymLink(loc_mtime, loc_hash))),
                    DeltaNode::Leaf(_, Some(FSNode::SymLink(miss_mtime, miss_hash))),
                ) => {
                    if loc_hash == miss_hash {
                        if loc_mtime != miss_mtime {
                            necessary_actions.push(
                                AbstPath::single(name),
                                Action::EditSymLink(miss_mtime.clone(), None),
                            );
                        }
                    } else {
                        return Err(getacterr(
                            AbstPath::single(name),
                            "added or edited incompatible symlinks with different endpoints",
                        ));
                    }
                }

                // Similar to the Branch-Branch branch
                (
                    DeltaNode::Leaf(_, Some(FSNode::Dir(_, _, loc_subtree))),
                    DeltaNode::Leaf(_, Some(FSNode::Dir(miss_mtime, _, miss_subtree))),
                ) => {
                    let subactions = add_tree_actions(loc_subtree, miss_subtree)
                        .map_err(push_getacterr(name))?;
                    necessary_actions.append(&mut subactions.add_prefix(name));
                    necessary_actions
                        .push(AbstPath::single(name), Action::EditDir(miss_mtime.clone()));
                }
                _ => {
                    return Err(getacterr(
                        AbstPath::single(name),
                        "added, edited or deleted incompatible objects",
                    ));
                }
            },
        }
    }
    Ok(necessary_actions)
}

#[cfg(test)]
mod tests {
    use super::{
        super::get_delta, add_tree_actions, get_actions, Action, Actions, Delta, DeltaNode, FSNode,
        FSTree,
    };
    use abst_fs::{AbstPath, Endpoint, Mtime};
    use core::panic;
    use std::{path::Path, vec};

    //--- UTILITY FUNCTIONS ---//
    fn add_dir_at(path: impl AsRef<Path>) -> (AbstPath, Action) {
        (AbstPath::from(path), Action::AddDir)
    }
    fn add_file_at(
        path: impl AsRef<Path>,
        mtime: (i64, u32),
        content: impl ToString,
    ) -> (AbstPath, Action) {
        (
            AbstPath::from(path),
            Action::AddFile(
                Mtime::from(mtime.0, mtime.1),
                hasher::hash_bytes(content.to_string().as_bytes()),
            ),
        )
    }
    fn add_symlink_at(
        path: impl AsRef<Path>,
        mtime: (i64, u32),
        endpoint: impl ToString,
    ) -> (AbstPath, Action) {
        (
            AbstPath::from(path),
            Action::AddSymLink(
                Mtime::from(mtime.0, mtime.1),
                hasher::hash_bytes(Endpoint::Unix(endpoint.to_string()).as_bytes()),
            ),
        )
    }
    fn edit_dir_at(path: impl AsRef<Path>, mtime: (i64, u32)) -> (AbstPath, Action) {
        (
            AbstPath::from(path),
            Action::EditDir(Mtime::from(mtime.0, mtime.1)),
        )
    }
    fn edit_file_at(
        path: impl AsRef<Path>,
        mtime: (i64, u32),
        content: Option<impl ToString>,
    ) -> (AbstPath, Action) {
        (
            AbstPath::from(path),
            Action::EditFile(
                Mtime::from(mtime.0, mtime.1),
                content.map(|val| hasher::hash_bytes(val.to_string().as_bytes())),
            ),
        )
    }
    fn edit_symlink_at(
        path: impl AsRef<Path>,
        mtime: (i64, u32),
        endpoint: Option<impl ToString>,
    ) -> (AbstPath, Action) {
        (
            AbstPath::from(path),
            Action::EditSymLink(
                Mtime::from(mtime.0, mtime.1),
                endpoint.map(|val| hasher::hash_bytes(Endpoint::Unix(val.to_string()).as_bytes())),
            ),
        )
    }
    fn remove_dir_at(path: impl AsRef<Path>) -> (AbstPath, Action) {
        (AbstPath::from(path), Action::RemoveDir)
    }
    fn remove_file_at(path: impl AsRef<Path>) -> (AbstPath, Action) {
        (AbstPath::from(path), Action::RemoveFile)
    }
    fn remove_symlink_at(path: impl AsRef<Path>) -> (AbstPath, Action) {
        (AbstPath::from(path), Action::RemoveSymLink)
    }
    //--- ---//

    #[test]
    fn various() {
        let action = Action::AddDir;
        let path = AbstPath::from("path/to/somewhere");
        let prefix = "some";
        let deep_path = path.add_first(prefix);

        let mut new_actions = Actions::new();

        assert_eq!(new_actions, Actions(Vec::new()));
        assert_ne!(new_actions, Actions(vec![(path.clone(), action.clone())]));

        new_actions.push(path.clone(), action.clone());
        assert_eq!(new_actions, Actions(vec![(path.clone(), action.clone())]));
        assert_ne!(new_actions, Actions(Vec::new()));

        let mut append_test = Actions::new();
        let mut some_actions = Actions(vec![(path.clone(), action.clone())]);
        append_test.append(&mut some_actions);
        assert_eq!(append_test, Actions(vec![(path, action.clone())]));
        assert_eq!(some_actions, Actions::new());

        assert_eq!(
            append_test.add_prefix(prefix),
            Actions(vec![(deep_path, action)])
        );

        for _ in Actions::new() {
            panic!("there should be no iterable");
        }

        let mut actions = Actions::new();
        actions.push(AbstPath::from("path/to/somewhere"), Action::RemoveDir);
        actions.push(AbstPath::from("another/path/here"), Action::RemoveSymLink);
        actions.push(AbstPath::from("yet/another/path"), Action::AddDir);

        let mut iter = (&actions).into_iter();
        assert_eq!(iter.next().unwrap(), &remove_dir_at("path/to/somewhere"));
        assert_eq!(
            iter.next().unwrap(),
            &remove_symlink_at("another/path/here")
        );
        assert_eq!(iter.next().unwrap(), &add_dir_at("yet/another/path"));
        assert_eq!(iter.next(), None);

        let mut iter = actions.into_iter();
        assert_eq!(iter.next().unwrap(), remove_dir_at("path/to/somewhere"));
        assert_eq!(iter.next().unwrap(), remove_symlink_at("another/path/here"));
        assert_eq!(iter.next().unwrap(), add_dir_at("yet/another/path"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_fs_to_actions() {
        assert_eq!(
            FSNode::file((1664660949, 951241393), "content").to_add_actions(),
            Actions(vec![add_file_at("", (1664660949, 951241393), "content")])
        );

        assert_eq!(
            FSNode::symlink((1664705309, 842419258), "path/to/somewhere").to_add_actions(),
            Actions(vec![add_symlink_at(
                "",
                (1664705309, 842419258),
                "path/to/somewhere"
            )])
        );

        assert_eq!(
            FSNode::dir((1664751233, 523727805), |t| {
                t.add_file("file", (1664660949, 951241393), "content");
                t.add_symlink("symlink", (1664705309, 842419258), "path/to/somewhere");
                t.add_empty_dir("dir", (1664707274, 116078511))
            })
            .to_add_actions(),
            Actions(vec![
                add_dir_at(""),
                add_file_at("file", (1664660949, 951241393), "content"),
                add_symlink_at("symlink", (1664705309, 842419258), "path/to/somewhere"),
                add_dir_at("dir"),
                edit_dir_at("dir", (1664707274, 116078511)),
                edit_dir_at("", (1664751233, 523727805))
            ])
        );
    }

    #[test]
    fn test_delta_remove_actions() {
        assert_eq!(
            DeltaNode::leaf(Some(FSNode::file((1665061768, 321204439), "test")), None)
                .to_actions()
                .unwrap(),
            Actions(vec![remove_file_at("")])
        );
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::symlink(
                    (1665233109, 187394758),
                    "path/to/somewhere"
                )),
                None
            )
            .to_actions()
            .unwrap(),
            Actions(vec![remove_symlink_at("")])
        );
        assert_eq!(
            DeltaNode::leaf(Some(FSNode::empty_dir((1665366613, 463960433))), None)
                .to_actions()
                .unwrap(),
            Actions(vec![remove_dir_at("")])
        );
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::dir((1665602584, 569209276), |t| {
                    t.add_file("some-file", (1665369992, 703846649), "content");
                    t.add_symlink("some-link", (1665476999, 523534123), "path/to/somewhere");
                    t.add_empty_dir("some-dir", (1665531191, 5096258));
                })),
                None
            )
            .to_actions()
            .unwrap(),
            Actions(vec![remove_dir_at("")])
        );
    }

    #[test]
    fn test_delta_add_actions() {
        assert_eq!(
            DeltaNode::leaf(None, Some(FSNode::file((1665061768, 321204439), "content")))
                .to_actions()
                .unwrap(),
            Actions(vec![add_file_at("", (1665061768, 321204439), "content")])
        );
        assert_eq!(
            DeltaNode::leaf(
                None,
                Some(FSNode::symlink(
                    (1665233109, 187394758),
                    "path/to/somewhere"
                ))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![add_symlink_at(
                "",
                (1665233109, 187394758),
                "path/to/somewhere"
            )])
        );
        assert_eq!(
            DeltaNode::leaf(None, Some(FSNode::empty_dir((1665366613, 463960433))))
                .to_actions()
                .unwrap(),
            Actions(vec![
                add_dir_at(""),
                edit_dir_at("", (1665366613, 463960433))
            ])
        );
        assert_eq!(
            DeltaNode::leaf(
                None,
                Some(FSNode::dir((1665602584, 569209276), |t| {
                    t.add_file("some-file", (1665369992, 703846649), "content");
                    t.add_symlink("some-link", (1665476999, 523534123), "path/to/somewhere");
                    t.add_empty_dir("some-dir", (1665531191, 5096258));
                })),
            )
            .to_actions()
            .unwrap(),
            Actions(vec![
                add_dir_at(""),
                add_file_at("some-file", (1665369992, 703846649), "content"),
                add_symlink_at("some-link", (1665476999, 523534123), "path/to/somewhere"),
                add_dir_at("some-dir"),
                edit_dir_at("some-dir", (1665531191, 5096258)),
                edit_dir_at("", (1665602584, 569209276))
            ])
        );
    }

    #[test]
    fn test_delta_edit_actions() {
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::file((1665639893, 998839999), "content 0")),
                Some(FSNode::file((1665646546, 757770519), "content 1"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![edit_file_at(
                "",
                (1665646546, 757770519),
                Some("content 1")
            )])
        );
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::file((1665639893, 998839999), "content 0")),
                Some(FSNode::file((1665639893, 998839999), "content 1"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![edit_file_at(
                "",
                (1665639893, 998839999),
                Some("content 1")
            )])
        );
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::file((1665639893, 998839999), "content 0")),
                Some(FSNode::file((1665646546, 757770519), "content 0"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![edit_file_at(
                "",
                (1665646546, 757770519),
                None::<String>
            )])
        );

        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::symlink((1665875820, 923687114), "some/path")),
                Some(FSNode::symlink((1665952290, 714857838), "different/path"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![edit_symlink_at(
                "",
                (1665952290, 714857838),
                Some("different/path")
            )])
        );
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::symlink((1665875820, 923687114), "some/path")),
                Some(FSNode::symlink((1665875820, 923687114), "different/path"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![edit_symlink_at(
                "",
                (1665875820, 923687114),
                Some("different/path")
            )])
        );
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::symlink((1665875820, 923687114), "some/path")),
                Some(FSNode::symlink((1665952290, 714857838), "some/path"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![edit_symlink_at(
                "",
                (1665952290, 714857838),
                None::<String>
            )])
        );
    }

    #[test]
    fn test_delta_mixed_actions() {
        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::file((1664807099, 335977847), "content")),
                Some(FSNode::symlink(
                    (1664809667, 879274895),
                    "path/to/somewhere"
                ))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![
                remove_file_at(""),
                add_symlink_at("", (1664809667, 879274895), "path/to/somewhere")
            ])
        );

        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::symlink(
                    (1664825242, 747002925),
                    "path/to/somewhere"
                )),
                Some(FSNode::file((1664957485, 518696027), "content"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![
                remove_symlink_at(""),
                add_file_at("", (1664957485, 518696027), "content")
            ])
        );

        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::file((1664957563, 520202425), "content 0")),
                Some(FSNode::dir((1665140704, 387057676), |t| {
                    t.add_file("file", (1665095557, 417992966), "content 1");
                    t.add_symlink("symlink", (1665109107, 135095944), "path/to/1");
                    t.add_empty_dir("dir", (1665117581, 211417004));
                }))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![
                remove_file_at(""),
                add_dir_at(""),
                add_file_at("file", (1665095557, 417992966), "content 1"),
                add_symlink_at("symlink", (1665109107, 135095944), "path/to/1"),
                add_dir_at("dir"),
                edit_dir_at("dir", (1665117581, 211417004)),
                edit_dir_at("", (1665140704, 387057676))
            ])
        );

        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::dir((1665373712, 119785857), |t| {
                    t.add_file("file", (1665302765, 574647081), "content 0");
                    t.add_symlink("symlink", (1665364925, 204697891), "path/to/0");
                    t.add_empty_dir("dir", (1665371147, 119546766));
                })),
                Some(FSNode::file((1665375981, 162851756), "content 1"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![
                remove_dir_at(""),
                add_file_at("", (1665375981, 162851756), "content 1")
            ])
        );

        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::symlink((1665379776, 876227613), "path/to/0")),
                Some(FSNode::dir((1665529202, 763408268), |t| {
                    t.add_file("file", (1665476739, 612391171), "content 1");
                    t.add_symlink("symlink", (1665491861, 186840443), "path/to/1");
                    t.add_empty_dir("dir", (1665523158, 944955427));
                }))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![
                remove_symlink_at(""),
                add_dir_at(""),
                add_file_at("file", (1665476739, 612391171), "content 1"),
                add_symlink_at("symlink", (1665491861, 186840443), "path/to/1"),
                add_dir_at("dir"),
                edit_dir_at("dir", (1665523158, 944955427)),
                edit_dir_at("", (1665529202, 763408268))
            ])
        );

        assert_eq!(
            DeltaNode::leaf(
                Some(FSNode::dir((1665642349, 362913905), |t| {
                    t.add_file("file", (1665541722, 958679428), "content 0");
                    t.add_symlink("symlink", (1665568082, 508646130), "path/to/0");
                    t.add_empty_dir("dir", (1665602418, 612036588));
                })),
                Some(FSNode::symlink((1665679053, 88025572), "path/to/1"))
            )
            .to_actions()
            .unwrap(),
            Actions(vec![
                remove_dir_at(""),
                add_symlink_at("", (1665679053, 88025572), "path/to/1")
            ])
        );
    }

    #[test]
    fn test_delta_branch_to_actions() {
        assert_eq!(
            DeltaNode::branch(((1665124842, 202898185), (1668146989, 229804914)), |d| {
                d.add_leaf(
                    "added-file",
                    None,
                    Some(FSNode::file((1667446876, 86247824), "content 0")),
                );
                d.add_leaf(
                    "added-symlink",
                    None,
                    Some(FSNode::symlink((1667473346, 507069866), "path/to/0")),
                );
                d.add_leaf(
                    "added-dir",
                    None,
                    Some(FSNode::dir((1667729035, 126919897), |t| {
                        t.add_file("some-file", (1667646083, 520484863), "content 1");
                        t.add_symlink("some-symlink", (1667668744, 385148336), "path/to/1");
                        t.add_empty_dir("some-dir", (1667696709, 72135293));
                    })),
                );

                d.add_leaf(
                    "removed-file",
                    Some(FSNode::file((1664587116, 614197665), "content 2")),
                    None,
                );
                d.add_leaf(
                    "removed-symlink",
                    Some(FSNode::symlink((1664596605, 681949735), "path/to/2")),
                    None,
                );
                d.add_leaf(
                    "removed-dir",
                    Some(FSNode::dir((1664720919, 551381051), |t| {
                        t.add_file("some-file", (1664600618, 565746625), "content 3");
                        t.add_symlink("some-symlink", (1664610876, 343494776), "path/to/3");
                        t.add_empty_dir("some-dir", (1664629786, 734851188));
                    })),
                    None,
                );

                d.add_leaf(
                    "edited-file",
                    Some(FSNode::file((1664778881, 140200307), "content 4")),
                    Some(FSNode::file((1667850556, 457134510), "content 5")),
                );
                d.add_leaf(
                    "edited-symlink",
                    Some(FSNode::symlink((1664825867, 451928681), "path/to/4")),
                    Some(FSNode::symlink((1667851286, 360603025), "path/to/5")),
                );

                d.add_branch(
                    "edited-dir",
                    ((1665117666, 829531525), (1668132614, 64460061)),
                    |d| {
                        d.add_leaf(
                            "old-file",
                            Some(FSNode::file((1664865241, 60667757), "content 6")),
                            None,
                        );
                        d.add_leaf(
                            "old-symlink",
                            Some(FSNode::symlink((1665039670, 630352010), "path/to/6")),
                            None,
                        );
                        d.add_leaf(
                            "old-dir",
                            Some(FSNode::empty_dir((1665110500, 590768331))),
                            None,
                        );
                        d.add_leaf(
                            "new-file",
                            None,
                            Some(FSNode::file((1668036281, 975073138), "content 7")),
                        );
                        d.add_leaf(
                            "new-symlink",
                            None,
                            Some(FSNode::symlink((1668083714, 158339310), "path/to/7")),
                        );
                        d.add_leaf(
                            "new-dir",
                            None,
                            Some(FSNode::empty_dir((1668094496, 259786048))),
                        );
                    },
                );
            })
            .to_actions()
            .unwrap(),
            Actions(vec![
                add_file_at("added-file", (1667446876, 86247824), "content 0"),
                add_symlink_at("added-symlink", (1667473346, 507069866), "path/to/0"),
                add_dir_at("added-dir"),
                add_file_at("added-dir/some-file", (1667646083, 520484863), "content 1"),
                add_symlink_at(
                    "added-dir/some-symlink",
                    (1667668744, 385148336),
                    "path/to/1"
                ),
                add_dir_at("added-dir/some-dir"),
                edit_dir_at("added-dir/some-dir", (1667696709, 72135293)),
                edit_dir_at("added-dir", (1667729035, 126919897)),
                remove_file_at("removed-file"),
                remove_symlink_at("removed-symlink"),
                remove_dir_at("removed-dir"),
                edit_file_at("edited-file", (1667850556, 457134510), Some("content 5")),
                edit_symlink_at("edited-symlink", (1667851286, 360603025), Some("path/to/5")),
                remove_file_at("edited-dir/old-file"),
                remove_symlink_at("edited-dir/old-symlink"),
                remove_dir_at("edited-dir/old-dir"),
                add_file_at("edited-dir/new-file", (1668036281, 975073138), "content 7"),
                add_symlink_at(
                    "edited-dir/new-symlink",
                    (1668083714, 158339310),
                    "path/to/7"
                ),
                add_dir_at("edited-dir/new-dir"),
                edit_dir_at("edited-dir/new-dir", (1668094496, 259786048)),
                edit_dir_at("edited-dir", (1668132614, 64460061)),
                edit_dir_at("", (1668146989, 229804914))
            ])
        );
    }

    #[test]
    fn test_to_actions_error() {
        assert!(DeltaNode::Leaf(None, None).to_actions().is_err());

        assert!(DeltaNode::Leaf(
            Some(FSNode::dir((1667282683, 412810936), |t| {
                t.add_file("file", (1667260690, 931913056), "content 0");
            })),
            Some(FSNode::dir((1667459385, 384123107), |t| {
                t.add_file("file", (1667371152, 845893374), "content 1")
            })),
        )
        .to_actions()
        .is_err());

        assert!(DeltaNode::Leaf(
            Some(FSNode::file((1667674660, 403784885), "content")),
            Some(FSNode::file((1667674660, 403784885), "content")),
        )
        .to_actions()
        .is_err());

        assert!(DeltaNode::Leaf(
            Some(FSNode::symlink(
                (1667717248, 635683372),
                "path/to/somewhere"
            )),
            Some(FSNode::symlink(
                (1667717248, 635683372),
                "path/to/somewhere"
            )),
        )
        .to_actions()
        .is_err());

        assert!(Delta::gen_from(|d| {
            d.add_leaf("non-existing-object", None, None);
        })
        .to_actions()
        .is_err());
    }

    #[test]
    fn test_add_tree_actions() {
        let loc_tree = FSTree::gen_from(|t| {
            t.add_file("local-file", (1667451494, 772117374), "content 0");
            t.add_file("both-file", (1667506143, 561334027), "content 1");
            t.add_symlink("local-symlink", (1667553005, 210771618), "path/to/2");
            t.add_symlink("both-symlink", (1667555532, 496471440), "path/to/3");
            t.add_dir("local-dir", (1667937343, 571153275), |t| {
                t.add_file("subfile", (1667561784, 175308734), "content 4");
                t.add_symlink("subsymlink", (1667592909, 802732313), "path/to/5");
                t.add_dir("subdir", (1667883595, 837320864), |t| {
                    t.add_file("subsubfile", (1667614538, 778078254), "content 6");
                    t.add_symlink("subsubsymlink", (1667773566, 563855678), "path/to/7");
                    t.add_empty_dir("subsubdir", (1667871619, 285711170));
                });
            });
            t.add_dir("both-dir", (1668484659, 516444151), |t| {
                t.add_file("local-subfile", (1667997942, 442065997), "content 8");
                t.add_file("both-subfile", (1668014918, 436747760), "content 9");
                t.add_symlink("local-subsymlink", (1668019428, 722513569), "path/to/10");
                t.add_symlink("both-subsymlink", (1668092665, 858853680), "path/to/11");
                t.add_dir("local-subdir", (1668216014, 599674193), |t| {
                    t.add_file("subsubfile", (1668097162, 380205), "content 12");
                    t.add_symlink("subsubsymlink", (1668171172, 802211979), "path/to/13");
                    t.add_empty_dir("subsubdir", (1668185830, 353575525));
                });
                t.add_dir("both-subdir", (1668477888, 934654008), |t| {
                    t.add_file("local-subsubfile", (1668219448, 309042254), "content 14");
                    t.add_file("both-subsubfile", (1668225374, 874901505), "content 15");
                    t.add_symlink("local-subsubsymlink", (1668338032, 654301301), "path/to/16");
                    t.add_symlink("both-subsubsymlink", (1668426639, 321572981), "path/to/17");
                    t.add_empty_dir("local-subsubdir", (1668459225, 672123212));
                    t.add_empty_dir("both-subsubdir", (1668461882, 992686858));
                });
            });
        });

        let miss_tree = FSTree::gen_from(|t| {
            t.add_file("miss-file", (1667440088, 512796633), "content 18");
            t.add_file("both-file", (1667457760, 877447014), "content 1");
            t.add_symlink("miss-symlink", (1667490289, 859903967), "path/to/19");
            t.add_symlink("both-symlink", (1667544335, 682787097), "path/to/3");
            t.add_dir("miss-dir", (1668078841, 677425226), |t| {
                t.add_file("subfile", (1667593277, 45544957), "content 20");
                t.add_symlink("subsymlink", (1667714924, 279596273), "path/to/21");
                t.add_dir("subdir", (1668030817, 816952290), |t| {
                    t.add_file("subsubfile", (1667861206, 21826977), "content 22");
                    t.add_symlink("subsubsymlink", (1667952070, 348725997), "path/to/23");
                    t.add_empty_dir("subsubdir", (1668004688, 520576083));
                });
            });
            t.add_dir("both-dir", (1668876296, 362301779), |t| {
                t.add_file("miss-subfile", (1668135999, 659914790), "content 24");
                t.add_file("both-subfile", (1668142965, 797805445), "content 9");
                t.add_symlink("miss-subsymlink", (1668152547, 534614451), "path/to/25");
                t.add_symlink("both-subsymlink", (1668180274, 853466233), "path/to/11");
                t.add_dir("miss-subdir", (1668458383, 978304854), |t| {
                    t.add_file("subsubfile", (1668291079, 678179687), "content 26");
                    t.add_symlink("subsubsymlink", (1668330452, 223848021), "path/to/27");
                    t.add_empty_dir("subsubdir", (1668368203, 10780309));
                });
                t.add_dir("both-subdir", (1668833003, 271190518), |t| {
                    t.add_file("miss-subsubfile", (1668509459, 862724084), "content 28");
                    t.add_file("both-subsubfile", (1668544464, 706471816), "content 15");
                    t.add_symlink("miss-subsubsymlink", (1668619831, 556739023), "path/to/29");
                    t.add_symlink("both-subsubsymlink", (1668650419, 227402875), "path/to/17");
                    t.add_empty_dir("miss-subsubdir", (1668743388, 316319405));
                    t.add_empty_dir("both-subsubdir", (1668758218, 914715759));
                });
            });
        });

        assert_eq!(
            add_tree_actions(&loc_tree, &miss_tree).unwrap(),
            Actions(vec![
                add_file_at("miss-file", (1667440088, 512796633), "content 18"),
                edit_file_at("both-file", (1667457760, 877447014), None::<String>),
                add_symlink_at("miss-symlink", (1667490289, 859903967), "path/to/19"),
                edit_symlink_at("both-symlink", (1667544335, 682787097), None::<String>),
                add_dir_at("miss-dir"),
                add_file_at("miss-dir/subfile", (1667593277, 45544957), "content 20"),
                add_symlink_at("miss-dir/subsymlink", (1667714924, 279596273), "path/to/21"),
                add_dir_at("miss-dir/subdir"),
                add_file_at(
                    "miss-dir/subdir/subsubfile",
                    (1667861206, 21826977),
                    "content 22"
                ),
                add_symlink_at(
                    "miss-dir/subdir/subsubsymlink",
                    (1667952070, 348725997),
                    "path/to/23"
                ),
                add_dir_at("miss-dir/subdir/subsubdir"),
                edit_dir_at("miss-dir/subdir/subsubdir", (1668004688, 520576083)),
                edit_dir_at("miss-dir/subdir", (1668030817, 816952290)),
                edit_dir_at("miss-dir", (1668078841, 677425226)),
                add_file_at(
                    "both-dir/miss-subfile",
                    (1668135999, 659914790),
                    "content 24"
                ),
                edit_file_at(
                    "both-dir/both-subfile",
                    (1668142965, 797805445),
                    None::<String>
                ),
                add_symlink_at(
                    "both-dir/miss-subsymlink",
                    (1668152547, 534614451),
                    "path/to/25"
                ),
                edit_symlink_at(
                    "both-dir/both-subsymlink",
                    (1668180274, 853466233),
                    None::<String>
                ),
                add_dir_at("both-dir/miss-subdir"),
                add_file_at(
                    "both-dir/miss-subdir/subsubfile",
                    (1668291079, 678179687),
                    "content 26"
                ),
                add_symlink_at(
                    "both-dir/miss-subdir/subsubsymlink",
                    (1668330452, 223848021),
                    "path/to/27"
                ),
                add_dir_at("both-dir/miss-subdir/subsubdir"),
                edit_dir_at("both-dir/miss-subdir/subsubdir", (1668368203, 10780309)),
                edit_dir_at("both-dir/miss-subdir", (1668458383, 978304854)),
                add_file_at(
                    "both-dir/both-subdir/miss-subsubfile",
                    (1668509459, 862724084),
                    "content 28"
                ),
                edit_file_at(
                    "both-dir/both-subdir/both-subsubfile",
                    (1668544464, 706471816),
                    None::<String>
                ),
                add_symlink_at(
                    "both-dir/both-subdir/miss-subsubsymlink",
                    (1668619831, 556739023),
                    "path/to/29"
                ),
                edit_symlink_at(
                    "both-dir/both-subdir/both-subsubsymlink",
                    (1668650419, 227402875),
                    None::<String>
                ),
                add_dir_at("both-dir/both-subdir/miss-subsubdir"),
                edit_dir_at(
                    "both-dir/both-subdir/miss-subsubdir",
                    (1668743388, 316319405)
                ),
                edit_dir_at(
                    "both-dir/both-subdir/both-subsubdir",
                    (1668758218, 914715759)
                ),
                edit_dir_at("both-dir/both-subdir", (1668833003, 271190518)),
                edit_dir_at("both-dir", (1668876296, 362301779))
            ])
        );
    }

    #[test]
    fn test_add_tree_actions_incompatible_files() {
        let loc_tree = FSTree::gen_from(|t| {
            t.add_file("file", (1667317389, 591254846), "content 1");
        });
        let miss_tree = FSTree::gen_from(|t| {
            t.add_file("file", (1667371999, 105275068), "content 2");
        });
        assert!(add_tree_actions(&loc_tree, &miss_tree).is_err());

        let loc_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1667388032, 851120567), |t| {
                t.add_file("file", (1667379354, 670238243), "content 1");
            })
        });
        let miss_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1667436647, 747771086), |t| {
                t.add_file("file", (1667397348, 246346603), "content 2");
            })
        });
        assert!(add_tree_actions(&loc_tree, &miss_tree).is_err());

        let loc_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1667561496, 117629802), |t| {
                t.add_dir("subdir", (1667474682, 640275839), |t| {
                    t.add_file("file", (1667468050, 995570173), "content 1");
                });
            })
        });
        let miss_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1667846654, 36544421), |t| {
                t.add_dir("subdir", (1667796732, 450055995), |t| {
                    t.add_file("file", (1667796104, 208959501), "content 2");
                });
            })
        });
        assert!(add_tree_actions(&loc_tree, &miss_tree).is_err());
    }

    #[test]
    fn test_add_tree_actions_incompatible_symlinks() {
        let loc_tree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1667878697, 159485180), "some/path/1");
        });
        let miss_tree = FSTree::gen_from(|t| {
            t.add_symlink("symlink", (1667944404, 882227232), "some/path/2");
        });
        assert!(add_tree_actions(&loc_tree, &miss_tree).is_err());

        let loc_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1668123900, 611195248), |t| {
                t.add_symlink("symlink", (1668034380, 842232587), "some/path/1");
            });
        });
        let miss_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1668171604, 892383389), |t| {
                t.add_symlink("symlink", (1668161075, 810573263), "some/path/2");
            });
        });
        assert!(add_tree_actions(&loc_tree, &miss_tree).is_err());

        let loc_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1668294682, 959268366), |t| {
                t.add_dir("subdir", (1668269796, 368947589), |t| {
                    t.add_symlink("symlink", (1668211342, 57892937), "some/path/1");
                });
            });
        });
        let miss_tree = FSTree::gen_from(|t| {
            t.add_dir("dir", (1668481091, 875126077), |t| {
                t.add_dir("subdir", (1668411639, 562698339), |t| {
                    t.add_symlink("symlink", (1668389605, 240974274), "some/path/2");
                });
            });
        });
        assert!(add_tree_actions(&loc_tree, &miss_tree).is_err());
    }

    #[test]
    fn test_add_tree_actions_incompatible_objects() {
        let incompatible_pairs = vec![
            (
                FSTree::gen_from(|t| {
                    t.add_file("object", (1664589312, 364269268), "content/that/is/path");
                }),
                FSTree::gen_from(|t| {
                    t.add_symlink("object", (1664589312, 364269268), "content/that/is/path");
                }),
            ),
            (
                FSTree::gen_from(|t| {
                    t.add_file("object", (1664656449, 979169705), "content");
                }),
                FSTree::gen_from(|t| {
                    t.add_empty_dir("object", (1664656449, 979169705));
                }),
            ),
            (
                FSTree::gen_from(|t| {
                    t.add_symlink("object", (1664673363, 54893229), "path/to/somewhere");
                }),
                FSTree::gen_from(|t| t.add_empty_dir("object", (1664673363, 54893229))),
            ),
        ];

        for (tree_a, tree_b) in incompatible_pairs {
            assert!(add_tree_actions(&tree_a, &tree_b).is_err());
            assert!(add_tree_actions(&tree_b, &tree_a).is_err());
        }
    }

    #[test]
    fn test_get_actions() {
        let original_tree = FSTree::gen_from(|t| {
            t.add_file("untouched-file", (1664618719, 438929376), "content 0");
            t.add_symlink("untouched-symlink", (1664647562, 210607085), "path/0");
            t.add_dir("untouched-dir", (1664887973, 407432140), |t| {
                t.add_file("subfile", (1664692710, 288974820), "content 1");
                t.add_symlink("subsymlink", (1664761576, 52649414), "path/1");
                t.add_empty_dir("subdir", (1664883562, 147534487))
            });

            t.add_file("local-removed-file", (1664961195, 468202222), "content 2");
            t.add_symlink("local-removed-symlink", (1665024534, 847430536), "path/2");
            t.add_dir("local-removed-dir", (1665261387, 231352611), |t| {
                t.add_file("subfile", (1665062187, 367155720), "content 3");
                t.add_symlink("subsymlink", (1665206422, 617080572), "path/3");
                t.add_empty_dir("subdir", (1665228487, 499369740));
            });

            t.add_file("missed-removed-file", (1665306994, 404772816), "content 4");
            t.add_symlink("missed-removed-symlink", (1665314193, 988233948), "path/4");
            t.add_dir("missed-removed-dir", (1665504908, 4978317), |t| {
                t.add_file("subfile", (1665325079, 948611217), "content 5");
                t.add_symlink("subsymlink", (1665352969, 460295244), "path/5");
                t.add_empty_dir("subdir", (1665452633, 478075251));
            });

            t.add_file("both-removed-file", (1665520430, 728538033), "content 6");
            t.add_symlink("both-removed-symlink", (1665560731, 907733727), "path/6");
            t.add_dir("both-removed-dir", (1665683917, 17572896), |t| {
                t.add_file("subdir", (1665660604, 837176848), "content 7");
                t.add_symlink("subsymlink", (1665675529, 200000766), "path/7");
                t.add_empty_dir("subdir", (1665681160, 941706584));
            });

            t.add_file("local-edited-file", (1665703624, 19884496), "content 8");
            t.add_symlink("local-edited-symlink", (1665747291, 452955016), "path/8");
            t.add_dir("local-edited-dir", (1665907509, 919419725), |t| {
                t.add_file("old-file", (1665777913, 98881941), "content 9");
                t.add_symlink("old-symlink", (1665789660, 602339612), "path/9");
                t.add_empty_dir("old-dir", (1665805210, 199579966));
            });

            t.add_file("missed-edited-file", (1665985641, 303939722), "content 10");
            t.add_symlink("missed-edited-symlink", (1666043231, 29982267), "path/10");
            t.add_dir("missed-edited-dir", (1666133096, 870109680), |t| {
                t.add_file("old-file", (1666054457, 506859804), "content 11");
                t.add_symlink("old-symlink", (1666062938, 717667145), "path/11");
                t.add_empty_dir("old-dir", (1666069431, 961199155));
            });

            t.add_file("both-edited-file", (1666180069, 318510756), "content 12");
            t.add_symlink("both-edited-symlink", (1666207184, 416305699), "path/12");
            t.add_dir("both-edited-dir", (1666241874, 50376), |t| {
                t.add_file("old-file", (1666210815, 195733470), "content 13");
                t.add_symlink("old-symlink", (1666226084, 482584832), "path/13");
                t.add_empty_dir("old-dir", (1666233174, 152383869));
            });
        });

        let local_tree = FSTree::gen_from(|t| {
            t.add_file("untouched-file", (1664618719, 438929376), "content 0");
            t.add_symlink("untouched-symlink", (1664647562, 210607085), "path/0");
            t.add_dir("untouched-dir", (1664887973, 407432140), |t| {
                t.add_file("subfile", (1664692710, 288974820), "content 1");
                t.add_symlink("subsymlink", (1664761576, 52649414), "path/1");
                t.add_empty_dir("subdir", (1664883562, 147534487))
            });

            t.add_file("local-added-file", (1667291501, 694997504), "content 14");
            t.add_symlink("local-added-symlink", (1667322009, 738125291), "path/14");
            t.add_dir("local-added-dir", (1667615484, 372298434), |t| {
                t.add_file("subfile", (1667405439, 30794745), "content 15");
                t.add_symlink("subsymlink", (1667415398, 455087769), "path/15");
                t.add_empty_dir("subdir", (1667604928, 338415406));
            });

            t.add_file("missed-removed-file", (1665306994, 404772816), "content 4");
            t.add_symlink("missed-removed-symlink", (1665314193, 988233948), "path/4");
            t.add_dir("missed-removed-dir", (1665504908, 4978317), |t| {
                t.add_file("subfile", (1665325079, 948611217), "content 5");
                t.add_symlink("subsymlink", (1665352969, 460295244), "path/5");
                t.add_empty_dir("subdir", (1665452633, 478075251));
            });

            t.add_file("both-added-file", (1667624266, 805140546), "content 16");
            t.add_symlink("both-added-symlink", (1667646560, 638616903), "path/16");
            t.add_dir("both-added-dir", (1667759241, 483275199), |t| {
                t.add_file("subfile", (1667674703, 972416754), "content 17");
                t.add_symlink("subsymlink", (1667715624, 937813631), "path/17");
                t.add_empty_dir("subdir", (1667742157, 769758628));
            });

            t.add_file("local-edited-file", (1667791301, 659357526), "content 18");
            t.add_symlink("local-edited-symlink", (1667856728, 649963249), "path/18");
            t.add_dir("local-edited-dir", (1667947748, 931134182), |t| {
                t.add_file("new-file", (1667858496, 748193565), "content 19");
                t.add_symlink("new-symlink", (1667864547, 496070096), "path/19");
                t.add_empty_dir("new-dir", (1667904471, 681801560));
            });

            t.add_file("missed-edited-file", (1665985641, 303939722), "content 10");
            t.add_symlink("missed-edited-symlink", (1666043231, 29982267), "path/10");
            t.add_dir("missed-edited-dir", (1666133096, 870109680), |t| {
                t.add_file("old-file", (1666054457, 506859804), "content 11");
                t.add_symlink("old-symlink", (1666062938, 717667145), "path/11");
                t.add_empty_dir("old-dir", (1666069431, 961199155));
            });

            t.add_file("both-edited-file", (1667951134, 121908092), "content 20");
            t.add_symlink("both-edited-symlink", (1667960130, 717688659), "path/20");
            t.add_dir("both-edited-dir", (1668024924, 991053051), |t| {
                t.add_file("new-file", (1667982527, 287017570), "content 21");
                t.add_symlink("new-symlink", (1667996499, 627515435), "path/21");
                t.add_empty_dir("new-dir", (1667999054, 767863400));
            });
        });

        let missed_tree = FSTree::gen_from(|t| {
            t.add_file("untouched-file", (1664618719, 438929376), "content 0");
            t.add_symlink("untouched-symlink", (1664647562, 210607085), "path/0");
            t.add_dir("untouched-dir", (1664887973, 407432140), |t| {
                t.add_file("subfile", (1664692710, 288974820), "content 1");
                t.add_symlink("subsymlink", (1664761576, 52649414), "path/1");
                t.add_empty_dir("subdir", (1664883562, 147534487))
            });

            t.add_file("local-removed-file", (1664961195, 468202222), "content 2");
            t.add_symlink("local-removed-symlink", (1665024534, 847430536), "path/2");
            t.add_dir("local-removed-dir", (1665261387, 231352611), |t| {
                t.add_file("subfile", (1665062187, 367155720), "content 3");
                t.add_symlink("subsymlink", (1665206422, 617080572), "path/3");
                t.add_empty_dir("subdir", (1665228487, 499369740));
            });

            t.add_file("missed-added-file", (1667273753, 118529591), "content 22");
            t.add_symlink("missed-added-symlink", (1667303887, 464385710), "path/22");
            t.add_dir("missed-added-dir", (1667550383, 256444912), |t| {
                t.add_file("subfile", (1667347766, 173666311), "content 23");
                t.add_symlink("subsymlink", (1667404817, 801485522), "path/23");
                t.add_empty_dir("subdir", (1667414556, 329098761));
            });

            t.add_file("both-added-file", (1667624227, 866162085), "content 16");
            t.add_symlink("both-added-symlink", (1667652496, 335642493), "path/16");
            t.add_dir("both-added-dir", (1667736237, 102383002), |t| {
                t.add_file("subfile", (1667669963, 626480240), "content 17");
                t.add_symlink("subsymlink", (1667681110, 459222078), "path/17");
                t.add_empty_dir("subdir", (1667705278, 500975522));
            });

            t.add_file("local-edited-file", (1665703624, 19884496), "content 8");
            t.add_symlink("local-edited-symlink", (1665747291, 452955016), "path/8");
            t.add_dir("local-edited-dir", (1665907509, 919419725), |t| {
                t.add_file("old-file", (1665777913, 98881941), "content 9");
                t.add_symlink("old-symlink", (1665789660, 602339612), "path/9");
                t.add_empty_dir("old-dir", (1665805210, 199579966));
            });

            t.add_file("missed-edited-file", (1667758112, 316412296), "content 24");
            t.add_symlink("missed-edited-symlink", (1667772261, 321012663), "path/24");
            t.add_dir("missed-edited-dir", (1667875880, 241898761), |t| {
                t.add_file("new-file", (1667824433, 642334672), "content 25");
                t.add_symlink("new-symlink", (1667865024, 181735718), "path/25");
                t.add_empty_dir("new-dir", (1667869683, 832041657));
            });

            t.add_file("both-edited-file", (1667959532, 32950243), "content 20");
            t.add_symlink("both-edited-symlink", (1667992821, 16282390), "path/20");
            t.add_dir("both-edited-dir", (1668038371, 400185901), |t| {
                t.add_file("new-file", (1668116249, 559633309), "content 21");
                t.add_symlink("new-symlink", (1668222709, 666338650), "path/21");
                t.add_empty_dir("new-dir", (1668279333, 510631155));
            });
        });

        let local_delta = get_delta(&original_tree, &local_tree);
        let missed_delta = get_delta(&original_tree, &missed_tree);

        let supposed_actions = Actions(vec![
            remove_file_at("missed-removed-file"),
            remove_symlink_at("missed-removed-symlink"),
            remove_dir_at("missed-removed-dir"),
            add_file_at("missed-added-file", (1667273753, 118529591), "content 22"),
            add_symlink_at("missed-added-symlink", (1667303887, 464385710), "path/22"),
            add_dir_at("missed-added-dir"),
            add_file_at(
                "missed-added-dir/subfile",
                (1667347766, 173666311),
                "content 23",
            ),
            add_symlink_at(
                "missed-added-dir/subsymlink",
                (1667404817, 801485522),
                "path/23",
            ),
            add_dir_at("missed-added-dir/subdir"),
            edit_dir_at("missed-added-dir/subdir", (1667414556, 329098761)),
            edit_dir_at("missed-added-dir", (1667550383, 256444912)),
            edit_file_at("both-added-file", (1667624227, 866162085), None::<String>),
            edit_symlink_at(
                "both-added-symlink",
                (1667652496, 335642493),
                None::<String>,
            ),
            edit_file_at(
                "both-added-dir/subfile",
                (1667669963, 626480240),
                None::<String>,
            ),
            edit_symlink_at(
                "both-added-dir/subsymlink",
                (1667681110, 459222078),
                None::<String>,
            ),
            edit_dir_at("both-added-dir/subdir", (1667705278, 500975522)),
            edit_dir_at("both-added-dir", (1667736237, 102383002)),
            edit_file_at(
                "missed-edited-file",
                (1667758112, 316412296),
                Some("content 24"),
            ),
            edit_symlink_at(
                "missed-edited-symlink",
                (1667772261, 321012663),
                Some("path/24"),
            ),
            remove_file_at("missed-edited-dir/old-file"),
            remove_symlink_at("missed-edited-dir/old-symlink"),
            remove_dir_at("missed-edited-dir/old-dir"),
            add_file_at(
                "missed-edited-dir/new-file",
                (1667824433, 642334672),
                "content 25",
            ),
            add_symlink_at(
                "missed-edited-dir/new-symlink",
                (1667865024, 181735718),
                "path/25",
            ),
            add_dir_at("missed-edited-dir/new-dir"),
            edit_dir_at("missed-edited-dir/new-dir", (1667869683, 832041657)),
            edit_dir_at("missed-edited-dir", (1667875880, 241898761)),
            edit_file_at("both-edited-file", (1667959532, 32950243), None::<String>),
            edit_symlink_at(
                "both-edited-symlink",
                (1667992821, 16282390),
                None::<String>,
            ),
            edit_file_at(
                "both-edited-dir/new-file",
                (1668116249, 559633309),
                None::<String>,
            ),
            edit_symlink_at(
                "both-edited-dir/new-symlink",
                (1668222709, 666338650),
                None::<String>,
            ),
            edit_dir_at("both-edited-dir/new-dir", (1668279333, 510631155)),
            edit_dir_at("both-edited-dir", (1668038371, 400185901)),
        ]);

        assert_eq!(
            get_actions(&local_delta, &missed_delta).unwrap(),
            supposed_actions
        );
    }

    #[test]
    fn test_get_actions_incompatible_add() {
        // incompatible file with different contents added
        let local_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "added-file",
                None,
                Some(FSNode::file((1664603545, 885795420), "content 0")),
            );
        });
        let missed_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "added-file",
                None,
                Some(FSNode::file((1664649428, 822180989), "content 1")),
            );
        });
        assert!(get_actions(&local_delta, &missed_delta).is_err());

        // incompatible symlink with different endpoints added
        let local_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "added-symlink",
                None,
                Some(FSNode::symlink((1664683403, 241602514), "path/to/0")),
            );
        });
        let missed_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "added-symlink",
                None,
                Some(FSNode::symlink((1664690880, 473009283), "path/to/1")),
            );
        });
        assert!(get_actions(&local_delta, &missed_delta).is_err());

        // incompatible dir with incompatible subtrees added
        let local_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "added-dir",
                None,
                Some(FSNode::dir((1664736282, 915421406), |t| {
                    t.add_file("file", (1664708191, 552506218), "content 0")
                })),
            );
        });
        let missed_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "added-dir",
                None,
                Some(FSNode::dir((1664788583, 147174387), |t| {
                    t.add_file("file", (1664753867, 965442226), "content 1")
                })),
            );
        });
        assert!(get_actions(&local_delta, &missed_delta).is_err());
    }

    #[test]
    fn test_get_actions_incompatible_removed_edited() {
        // file removed in one delta and edited in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                Some(FSNode::file((1665063299, 285607992), "content 0")),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "file",
                Some(FSNode::file((1665063299, 285607992), "content 0")),
                Some(FSNode::file((1665090622, 597598751), "content 1")),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // symlink removed in one delta and edited in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                Some(FSNode::symlink((1665196940, 119162612), "path/to/0")),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "symlink",
                Some(FSNode::symlink((1665196940, 119162612), "path/to/0")),
                Some(FSNode::symlink((1665223273, 760578799), "path/to/1")),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // directory removed in one delta and edited in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "dir",
                Some(FSNode::dir((1665331950, 471877562), |t| {
                    t.add_file("file", (1665263460, 590921524), "content 0");
                    t.add_symlink("old-symlink", (1665301507, 187973255), "path/to/0");
                })),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_branch(
                "dir",
                ((1665331950, 471877562), (1665402229, 55735766)),
                |d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1665263460, 590921524), "content 0")),
                        Some(FSNode::file((1665363002, 677215165), "content 1")),
                    );
                    d.add_leaf(
                        "old-symlink",
                        Some(FSNode::symlink((1665301507, 187973255), "path/to/0")),
                        None,
                    );
                    d.add_leaf(
                        "new-symlink",
                        None,
                        Some(FSNode::symlink((1665394458, 120284), "path/to/1")),
                    );
                },
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());
    }

    #[test]
    fn test_get_actions_incompatible_removed_transmuted() {
        // file removed in one delta and transmuted to symlink in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665478902, 93299644), "content")),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665478902, 93299644), "content")),
                Some(FSNode::symlink(
                    (1665514202, 772612222),
                    "path/to/somewhere",
                )),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // file removed in one delta and transmuted to dir in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665561302, 495615752), "content 0")),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1665561302, 495615752), "content 0")),
                Some(FSNode::dir((1665651032, 835157963), |t| {
                    t.add_file("file", (1665580596, 827336505), "content 1");
                    t.add_symlink("symlink", (1665602521, 458444935), "path/to/1");
                    t.add_empty_dir("dir", (1665626429, 78139615));
                })),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // symlink removed in one delta and transmuted to file in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink(
                    (1665657237, 433270969),
                    "path/to/somewhere",
                )),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink(
                    (1665657237, 433270969),
                    "path/to/somewhere",
                )),
                Some(FSNode::file((1665673849, 235576552), "content")),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // symlink removed in one delta and transmuted to dir in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665702117, 763912389), "path/to/0")),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1665702117, 763912389), "path/to/0")),
                Some(FSNode::dir((1665816272, 227262844), |t| {
                    t.add_file("file", (1665719233, 186279280), "content 1");
                    t.add_symlink("symlink", (1665745566, 924656978), "path/to/1");
                    t.add_empty_dir("dir", (1665785494, 838031762));
                })),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // dir removed in one delta and transmuted to file in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665918603, 271333372), |t| {
                    t.add_file("file", (1665839770, 938267177), "content 1");
                    t.add_symlink("symlink", (1665867159, 374669586), "path/to/1");
                    t.add_empty_dir("dir", (1665910959, 156274683));
                })),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1665918603, 271333372), |t| {
                    t.add_file("file", (1665839770, 938267177), "content 1");
                    t.add_symlink("symlink", (1665867159, 374669586), "path/to/1");
                    t.add_empty_dir("dir", (1665910959, 156274683));
                })),
                Some(FSNode::file((1665928508, 128117502), "content")),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // dir removed in one delta and transmuted to symlink in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1666249983, 748990368), |t| {
                    t.add_file("file", (1665942771, 544580287), "content 1");
                    t.add_symlink("symlink", (1665948940, 30221650), "path/to/1");
                    t.add_empty_dir("dir", (1666228141, 634730726));
                })),
                None,
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1666249983, 748990368), |t| {
                    t.add_file("file", (1665942771, 544580287), "content 1");
                    t.add_symlink("symlink", (1665948940, 30221650), "path/to/1");
                    t.add_empty_dir("dir", (1666228141, 634730726));
                })),
                Some(FSNode::symlink(
                    (1666289673, 190610684),
                    "path/to/somewhere",
                )),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());
    }

    #[test]
    fn test_get_actions_incompatible_edited_transmuted() {
        // file edited in one delta and transmuted to symlink in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1666308496, 7434939), "content 0")),
                Some(FSNode::file((1666325737, 819621176), "content 1")),
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1666308496, 7434939), "content 0")),
                Some(FSNode::symlink((1666343124, 606726773), "path/to/1")),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // file edited in one delta and transmuted to dir in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1666390089, 911974343), "content 0")),
                Some(FSNode::file((1666427602, 476224360), "content 1")),
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::file((1666390089, 911974343), "content 0")),
                Some(FSNode::dir((1666514773, 515712216), |t| {
                    t.add_file("file", (1666477568, 32287767), "contet 2");
                    t.add_symlink("symlink", (1666483069, 736884486), "path/to/2");
                    t.add_empty_dir("dir", (1666502790, 842881410));
                })),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // symlink edited in one delta and transmuted to file in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1666561667, 807783399), "path/to/0")),
                Some(FSNode::symlink((1666609819, 158459424), "path/to/1")),
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1666561667, 807783399), "path/to/0")),
                Some(FSNode::file((1666617070, 849204390), "content 1")),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // symlink edited in one delta and transmuted to dir in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1666658977, 599996092), "path/to/0")),
                Some(FSNode::symlink((1666667185, 127792820), "path/to/1")),
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::symlink((1666658977, 599996092), "path/to/0")),
                Some(FSNode::dir((1666802421, 937157190), |t| {
                    t.add_file("file", (1666710573, 139243791), "content 2");
                    t.add_symlink("symlink", (1666758818, 22536578), "path/to/2");
                    t.add_empty_dir("dir", (1666790954, 71823044));
                })),
            );
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // dir edited in one delta and transmute to file in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_branch(
                "object",
                ((1666874148, 320740614), (1666914869, 444762343)),
                |d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1666831953, 370167403), "content 0")),
                        None,
                    );
                },
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1666874148, 320740614), |t| {
                    t.add_file("file", (1666831953, 370167403), "content 0");
                })),
                Some(FSNode::file((1666930691, 687284154), "content 1")),
            )
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());

        // dir edited in one delta and transmute to symlink in the other
        let delta_a = Delta::gen_from(|d| {
            d.add_branch(
                "object",
                ((1666968301, 117225472), (1667012757, 884956853)),
                |d| {
                    d.add_leaf(
                        "file",
                        Some(FSNode::file((1666939577, 238422267), "content 0")),
                        None,
                    );
                },
            );
        });
        let delta_b = Delta::gen_from(|d| {
            d.add_leaf(
                "object",
                Some(FSNode::dir((1666968301, 117225472), |t| {
                    t.add_file("file", (1666939577, 238422267), "content 0");
                })),
                Some(FSNode::symlink((1667027833, 689918590), "path/to/1")),
            )
        });
        assert!(get_actions(&delta_a, &delta_b).is_err());
        assert!(get_actions(&delta_b, &delta_a).is_err());
    }

    #[test]
    fn test_get_actions_incompatible_other() {
        // missed delta has locally-unchanged object but is not shaken
        let local_delta = Delta::empty();
        let missed_delta = Delta::gen_from(|d| {
            d.add_leaf(
                "unshaken-leaf",
                Some(FSNode::file((1664597221, 990157477), "content")),
                Some(FSNode::file((1664597221, 990157477), "content")),
            );
        });
        assert!(get_actions(&local_delta, &missed_delta).is_err());

        // nested error
        let local_delta = Delta::gen_from(|d| {
            d.add_branch(
                "edited-dir",
                ((1664925082, 715961036), (1664973763, 621623329)),
                |d| {
                    d.add_leaf(
                        "added-file",
                        None,
                        Some(FSNode::file((1664942343, 812950714), "content 0")),
                    );
                },
            );
        });
        let missed_delta = Delta::gen_from(|d| {
            d.add_branch(
                "edited-dir",
                ((1664925082, 715961036), (1665046376, 720283471)),
                |d| {
                    d.add_leaf(
                        "added-file",
                        None,
                        Some(FSNode::file((1665014258, 471772924), "content 1")),
                    )
                },
            );
        });
        assert!(get_actions(&local_delta, &missed_delta).is_err());
    }
}
