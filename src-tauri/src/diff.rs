//! Structural before/after diff. Matches nodes by (kind, name) via LCS so order
//! is preserved and add/remove/modify fall out naturally. A node whose text only
//! differs in whitespace is treated as unchanged (pure formatting).
//!
//! Ids are assigned by `assign_ids` AFTER diffing — structure-derived (file / parent
//! path / kind / name), never from body text, so editing a node's body keeps its id
//! and the frontend selection survives live re-analysis.
use std::collections::HashMap;

use crate::model::{AstNode, NodeState};
use crate::parse::Parsed;

pub fn diff_nodes(before: &[Parsed], after: &[Parsed]) -> Vec<AstNode> {
    let bkeys: Vec<_> = before.iter().map(|p| p.key()).collect();
    let akeys: Vec<_> = after.iter().map(|p| p.key()).collect();
    let matches = lcs_matches(&bkeys, &akeys);

    let mut out = Vec::new();
    let (mut bi, mut ai, mut mi) = (0usize, 0usize, 0usize);
    loop {
        if let Some(&(mb, ma)) = matches.get(mi) {
            while bi < mb {
                out.push(removed(&before[bi]));
                bi += 1;
            }
            while ai < ma {
                out.push(added(&after[ai]));
                ai += 1;
            }
            out.push(matched(&before[mb], &after[ma]));
            bi = mb + 1;
            ai = ma + 1;
            mi += 1;
        } else {
            while bi < before.len() {
                out.push(removed(&before[bi]));
                bi += 1;
            }
            while ai < after.len() {
                out.push(added(&after[ai]));
                ai += 1;
            }
            break;
        }
    }
    out
}

fn matched(b: &Parsed, a: &Parsed) -> AstNode {
    let has_children = !b.children.is_empty() || !a.children.is_empty();
    if has_children {
        // Container (e.g. a function): hold the diffed children but stay calm; drop
        // them entirely if nothing inside changed so it renders as a plain card.
        let children = diff_nodes(&b.children, &a.children);
        let any_changed = children.iter().any(|c| c.state != NodeState::Unchanged);
        node(a, NodeState::Unchanged, None, None, if any_changed { Some(children) } else { None })
    } else if normalize(&b.lines) != normalize(&a.lines) {
        node(a, NodeState::Modified, Some(b.lines.clone()), Some(a.lines.clone()), None)
    } else {
        node(a, NodeState::Unchanged, None, None, None)
    }
}

fn added(a: &Parsed) -> AstNode {
    node(a, NodeState::Added, None, Some(a.lines.clone()), None)
}

fn removed(b: &Parsed) -> AstNode {
    node(b, NodeState::Removed, Some(b.lines.clone()), None, None)
}

fn node(
    p: &Parsed,
    state: NodeState,
    before: Option<Vec<String>>,
    after: Option<Vec<String>>,
    children: Option<Vec<AstNode>>,
) -> AstNode {
    AstNode {
        id: String::new(), // assigned later by `assign_ids`
        kind: p.kind.clone(),
        name: p.name.clone(),
        signature: p.signature.clone(),
        state,
        flag_id: None,
        before,
        after,
        children,
    }
}

/// Assign structure-derived ids: `{fileId}:{parentPath}:{kind}:{nameSlug}`, with a
/// `#idx` suffix ONLY when siblings share the same (kind, name) — so unique named
/// declarations get stable ids regardless of position; only anonymous/duplicate
/// statements fall back to a positional index.
pub fn assign_ids(nodes: &mut [AstNode], file_id: &str, parent_path: &str) {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for n in nodes.iter() {
        *counts.entry((n.kind.clone(), slug(&n.name))).or_insert(0) += 1;
    }
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    for n in nodes.iter_mut() {
        let s = slug(&n.name);
        let key = (n.kind.clone(), s.clone());
        let base = format!("{file_id}:{parent_path}:{}:{}", n.kind, s);
        n.id = if counts[&key] > 1 {
            let i = seen.entry(key).or_insert(0);
            let cur = *i;
            *i += 1;
            format!("{base}#{cur}")
        } else {
            base
        };
        if let Some(children) = n.children.as_mut() {
            let child_parent = format!("{parent_path}/{s}");
            assign_ids(children, file_id, &child_parent);
        }
    }
}

fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    s.chars().take(24).collect()
}

/// Collapse all whitespace so two blocks that differ only in formatting compare equal.
fn normalize(lines: &[String]) -> String {
    lines.join("\n").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Longest common subsequence of the key sequences → matched (before_i, after_j) pairs.
fn lcs_matches(a: &[(String, String)], b: &[(String, String)]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}
