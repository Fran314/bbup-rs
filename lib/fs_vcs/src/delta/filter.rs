use super::{AbstPath, Delta, DeltaNode, ExcludeList, FSNode};

impl Delta {
    // TODO maybe these should return something about what they have filtered out?
    pub fn filter_out(&mut self, exclude_list: &ExcludeList) {
        self.filter_out_rec(&AbstPath::single("."), exclude_list);
    }
    fn filter_out_rec(&mut self, rel_path: &AbstPath, exclude_list: &ExcludeList) {
        let Delta(tree) = self;
        for (name, child) in tree {
            match child {
                DeltaNode::Leaf(pre, post) => {
                    if exclude_list.should_exclude(
                        &rel_path.add_last(name),
                        matches!(pre, Some(FSNode::Dir(_, _, _))),
                    ) {
                        *pre = None;
                    }

                    if exclude_list.should_exclude(
                        &rel_path.add_last(name),
                        matches!(post, Some(FSNode::Dir(_, _, _))),
                    ) {
                        *post = None;
                    }
                }
                DeltaNode::Branch(optm, subdelta) => {
                    if exclude_list.should_exclude(&rel_path.add_last(name), true) {
                        // Make it so that the branch will be removed once the
                        //	delta gets shaken at the end of the function
                        *optm = None;
                        *subdelta = Delta::empty();
                    } else {
                        subdelta.filter_out_rec(&rel_path.add_last(name), exclude_list);
                    }
                }
            }
        }
        self.shake();
    }
}
