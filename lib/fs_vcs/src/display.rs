use super::{ConflictNode, Conflicts, Delta, DeltaNode, FSNode, FSTree};

use colored::Color;
use colored::Colorize;

struct StringTree {
    text: String,
    children: Vec<StringTree>,
}

impl StringTree {
    fn leaf<S: std::string::ToString>(text: S) -> StringTree {
        StringTree {
            text: text.to_string(),
            children: Vec::new(),
        }
    }
}

impl std::fmt::Display for StringTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = if self.children.is_empty() {
            self.text.clone()
        } else {
            let mut blocks: Vec<Vec<String>> = Vec::new();
            for child in &self.children {
                let block: Vec<String> = format!("{}", child)
                    .lines()
                    .map(|line| line.to_string())
                    .collect();
                blocks.push(block);
            }

            for i in 0..blocks.len() {
                let (first_indent, mid_indent) = match i < blocks.len() - 1 {
                    true => ("├── ", "│   "),
                    false => ("└── ", "    "),
                };
                blocks[i][0] = format!("{}{}", first_indent, blocks[i][0]);
                for j in 1..blocks[i].len() {
                    blocks[i][j] = format!("{}{}", mid_indent, blocks[i][j]);
                }
            }

            self.text.clone()
                + "\n"
                + blocks
                    .into_iter()
                    .map(|block| block.join("\n"))
                    .collect::<Vec<String>>()
                    .join("\n")
                    .as_str()
        };
        write!(f, "{}", text)
    }
}

fn styled<S: std::string::ToString, C: Into<Color>>(text: S, color: C) -> String {
    text.to_string().color(color).to_string()
}
fn styled_dir<S: std::string::ToString, C: Into<Color>>(text: S, color: C) -> String {
    styled(text.to_string() + "/", color)
}
fn typed<S: std::string::ToString>(t: &str, text: S) -> String {
    format!("[{}] {}", t, text.to_string())
}
fn fstree_to_stringtree<S: std::string::ToString, C: Clone + Into<Color>>(
    root_text: S,
    FSTree(tree): &FSTree,
    color: C,
) -> StringTree {
    let mut children = tree.iter().collect::<Vec<(&String, &FSNode)>>();
    children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));
    StringTree {
        text: root_text.to_string(),
        children: children
            .into_iter()
            .map(|(name, child)| match child {
                FSNode::File(_, _) => {
                    let name = styled(name, color.clone());
                    StringTree::leaf(typed("f", name))
                }
                FSNode::SymLink(_, _) => {
                    let name = styled(name, color.clone());
                    StringTree::leaf(typed("s", name))
                }
                FSNode::Dir(_, _, subtree) => {
                    let text = styled(name.clone() + "/", color.clone());
                    fstree_to_stringtree(typed("d", text), subtree, color.clone())
                }
            })
            .collect(),
    }
}

impl std::fmt::Display for FSTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", fstree_to_stringtree(".", self, ""))
    }
}

fn deltafstree_to_stringtree<S: std::string::ToString>(
    root_text: S,
    Delta(tree): &Delta,
) -> StringTree {
    use DeltaNode::*;
    use FSNode::*;
    let mut children = tree.iter().collect::<Vec<(&String, &DeltaNode)>>();
    children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));

    StringTree {
        text: root_text.to_string(),
        children: children
            .into_iter()
            .flat_map(|(name, child)| match child {
                Branch(optm, subdelta) => {
                    let color = match optm {
                        Some(_) => "yellow",
                        None => "",
                    };
                    let Delta(subtree) = subdelta;
                    if !subtree.is_empty() {
                        vec![deltafstree_to_stringtree(
                            typed("d", styled_dir(name, color)),
                            subdelta,
                        )]
                    } else {
                        vec![StringTree::leaf(typed("d", styled_dir(name, color)))]
                    }
                }
                Leaf(Some(File(_, _)), Some(File(_, _))) => {
                    vec![StringTree::leaf(typed("f", styled(name, "yellow")))]
                }
                Leaf(Some(SymLink(_, _)), Some(SymLink(_, _))) => {
                    vec![StringTree::leaf(typed("s", styled(name, "yellow")))]
                }
                // I don't really think it's necessary to check that pre != post,
                //	it's only a display utility function and we """know""" we're
                //	only working with shaken trees so the only place were this
                //	could be working on unshaken trees is on malicious code, and
                //	fuck them.
                // Still, I'll leave the pre != post just to be sure
                Leaf(pre, post) if pre != post => {
                    let mut output = vec![];
                    if let Some(val) = pre {
                        let removed = match val {
                            File(_, _) => StringTree::leaf(typed("f", styled(name, "red"))),
                            SymLink(_, _) => StringTree::leaf(typed("s", styled(name, "red"))),
                            Dir(_, _, subtree) => fstree_to_stringtree(
                                typed("d", styled_dir(name, "red")),
                                subtree,
                                "red",
                            ),
                        };
                        output.push(removed);
                    }
                    if let Some(val) = post {
                        let added = match val {
                            File(_, _) => StringTree::leaf(typed("f", styled(name, "green"))),
                            SymLink(_, _) => StringTree::leaf(typed("s", styled(name, "green"))),
                            Dir(_, _, subtree) => fstree_to_stringtree(
                                typed("d", styled_dir(name, "green")),
                                subtree,
                                "green",
                            ),
                        };
                        output.push(added);
                    }

                    output
                }
                _ => {
                    vec![StringTree::leaf(format!(
                        "{}??? - this node should have been shaken",
                        name
                    ))]
                }
            })
            .collect(),
    }
}

impl std::fmt::Display for Delta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", deltafstree_to_stringtree(".", self))
    }
}

fn format_leaf_state(val: &Option<FSNode>) -> String {
    match val {
        Some(FSNode::File(_, hash)) => {
            format!("File [h:{}]", hash.to_hex(6),)
        }
        Some(FSNode::SymLink(_, hash)) => {
            format!("SymLink [h:{}]", hash.to_hex(6),)
        }
        Some(FSNode::Dir(_, hash, _)) => {
            format!("Dir [h:{}]", hash.to_hex(6),)
        }
        None => String::from("None"),
    }
}
fn format_delta_leaf(val: &DeltaNode) -> String {
    match val {
        DeltaNode::Leaf(pre0, post0) => {
            format!(
                "{} -> {}",
                format_leaf_state(pre0),
                format_leaf_state(post0)
            )
        }
        DeltaNode::Branch(_, _) => String::from("Dir [inner modification]"),
    }
}
fn conflicts_to_stringtree<S: std::string::ToString>(
    text: S,
    Conflicts(tree): &Conflicts,
) -> StringTree {
    use ConflictNode as CN;
    use DeltaNode as DN;
    use FSNode as FN;

    let mut children = tree.iter().collect::<Vec<(&String, &ConflictNode)>>();
    children.sort_by(|(name0, _), (name1, _)| name0.cmp(name1));

    StringTree {
        text: text.to_string(),
        children: children
            .into_iter()
            .map(|(name, child)| match child {
                CN::Leaf(delta0, delta1) => match (delta0, delta1) {
                    (DN::Leaf(_, Some(FN::Dir(_, _, _))), DN::Leaf(_, Some(FN::Dir(_, _, _)))) => {
                        StringTree::leaf(format!(
                            "{}\n0: {}\n1: {}\nwith incompatible subtrees",
                            name,
                            format_delta_leaf(delta0),
                            format_delta_leaf(delta1)
                        ))
                    }
                    _ => StringTree::leaf(format!(
                        "{}\n0: {}\n1: {}",
                        name,
                        format_delta_leaf(delta0),
                        format_delta_leaf(delta1)
                    )),
                },
                CN::Branch(subconflicts) => {
                    conflicts_to_stringtree(format!("{}/", name), subconflicts)
                }
            })
            .collect(),
    }
}

impl std::fmt::Display for Conflicts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", conflicts_to_stringtree(".", self))
    }
}
