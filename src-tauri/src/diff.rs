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
    let lines_changed = normalize(&b.lines) != normalize(&a.lines);
    if has_children {
        // Containers (e.g. functions) can change at the header/signature and/or
        // inside the body. Keep unchanged children hidden so a signature-only edit
        // still renders as one focused modified card.
        let children = diff_nodes(&b.children, &a.children);
        let any_changed = children.iter().any(|c| c.state != NodeState::Unchanged);
        let signature_changed = b.signature != a.signature && (b.signature.is_some() || a.signature.is_some());
        node(
            a,
            if signature_changed {
                NodeState::Modified
            } else {
                NodeState::Unchanged
            },
            if signature_changed { Some(b.lines.clone()) } else { None },
            if signature_changed { Some(a.lines.clone()) } else { None },
            if any_changed { Some(children) } else { None },
        )
    } else if lines_changed {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn p(kind: &str, name: &str, lines: &[&str]) -> Parsed {
        Parsed {
            kind: kind.into(),
            name: name.into(),
            signature: None,
            lines: lines.iter().map(|s| s.to_string()).collect(),
            children: Vec::new(),
        }
    }

    fn states(nodes: &[AstNode]) -> Vec<(String, NodeState)> {
        nodes.iter().map(|n| (n.name.clone(), n.state)).collect()
    }

    #[test]
    fn detects_added_removed_modified_unchanged() {
        let before = vec![
            p("VariableDeclaration", "kept", &["const kept = 1;"]),
            p("VariableDeclaration", "gone", &["const gone = 2;"]),
            p("VariableDeclaration", "edited", &["const edited = 3;"]),
        ];
        let after = vec![
            p("VariableDeclaration", "kept", &["const kept = 1;"]),
            p("VariableDeclaration", "edited", &["const edited = 99;"]),
            p("VariableDeclaration", "fresh", &["const fresh = 4;"]),
        ];
        let out = diff_nodes(&before, &after);
        assert_eq!(
            states(&out),
            vec![
                ("kept".to_string(), NodeState::Unchanged),
                ("gone".to_string(), NodeState::Removed),
                ("edited".to_string(), NodeState::Modified),
                ("fresh".to_string(), NodeState::Added),
            ]
        );
        let edited = &out[2];
        assert_eq!(edited.before.as_deref(), Some(&["const edited = 3;".to_string()][..]));
        assert_eq!(edited.after.as_deref(), Some(&["const edited = 99;".to_string()][..]));
        let gone = &out[1];
        assert!(gone.before.is_some() && gone.after.is_none());
        let fresh = &out[3];
        assert!(fresh.before.is_none() && fresh.after.is_some());
    }

    #[test]
    fn whitespace_only_difference_is_unchanged() {
        let before = vec![p("VariableDeclaration", "x", &["const x = {", "  a: 1,", "};"])];
        let after = vec![p("VariableDeclaration", "x", &["const x = {  a: 1, };"])];
        let out = diff_nodes(&before, &after);
        assert_eq!(out[0].state, NodeState::Unchanged);
        assert!(out[0].before.is_none() && out[0].after.is_none());
    }

    #[test]
    fn container_keeps_children_only_when_something_inside_changed() {
        let mut fn_before = p("FunctionDeclaration", "run", &["function run() {}"]);
        fn_before.children = vec![p("ReturnStatement", "return", &["return 1;"])];
        let mut fn_after_same = fn_before.clone();
        fn_after_same.lines = vec!["function run()  {}".into()]; // container text irrelevant
        let out = diff_nodes(&[fn_before.clone()], &[fn_after_same]);
        assert_eq!(out[0].state, NodeState::Unchanged);
        assert!(out[0].children.is_none(), "calm container drops children");

        let mut fn_after_edited = fn_before.clone();
        fn_after_edited.children = vec![p("ReturnStatement", "return", &["return 2;"])];
        let out = diff_nodes(&[fn_before], &[fn_after_edited]);
        assert_eq!(out[0].state, NodeState::Unchanged, "container stays calm");
        let children = out[0].children.as_ref().expect("children kept");
        assert_eq!(children[0].state, NodeState::Modified);
    }

    #[test]
    fn container_signature_only_change_marks_container_modified() {
        let mut fn_before = p(
            "FunctionDeclaration",
            "run",
            &["function run() {", "  return 1;", "}"],
        );
        fn_before.signature = Some("()".into());
        fn_before.children = vec![p("ReturnStatement", "return", &["return 1;"])];

        let mut fn_after = p(
            "FunctionDeclaration",
            "run",
            &["function run(param: any) {", "  return 1;", "}"],
        );
        fn_after.signature = Some("(param: any)".into());
        fn_after.children = vec![p("ReturnStatement", "return", &["return 1;"])];

        let out = diff_nodes(&[fn_before], &[fn_after]);
        assert_eq!(out[0].state, NodeState::Modified);
        assert_eq!(
            out[0].before.as_deref(),
            Some(&[
                "function run() {".to_string(),
                "  return 1;".to_string(),
                "}".to_string()
            ][..])
        );
        assert_eq!(
            out[0].after.as_deref(),
            Some(&[
                "function run(param: any) {".to_string(),
                "  return 1;".to_string(),
                "}".to_string()
            ][..])
        );
        assert!(out[0].children.is_none(), "unchanged body stays visually calm");
    }

    #[test]
    fn ids_are_stable_and_disambiguate_duplicates() {
        let mut nodes = diff_nodes(
            &[],
            &[
                p("VariableDeclaration", "unique", &["const unique = 1;"]),
                p("ExpressionStatement", "doIt()", &["doIt();"]),
                p("ExpressionStatement", "doIt()", &["doIt();"]),
            ],
        );
        assign_ids(&mut nodes, "file", "");
        assert_eq!(nodes[0].id, "file::VariableDeclaration:unique", "unique name → no index");
        assert_eq!(nodes[1].id, "file::ExpressionStatement:doIt__#0");
        assert_eq!(nodes[2].id, "file::ExpressionStatement:doIt__#1");
    }

    #[test]
    fn child_ids_include_parent_path() {
        let mut fn_node = p("FunctionDeclaration", "run", &["function run() {}"]);
        fn_node.children = vec![p("ReturnStatement", "return", &["return 2;"])];
        let mut prev = fn_node.clone();
        prev.children = vec![p("ReturnStatement", "return", &["return 1;"])];
        let mut nodes = diff_nodes(&[prev], &[fn_node]);
        assign_ids(&mut nodes, "file", "");
        let child = &nodes[0].children.as_ref().unwrap()[0];
        assert_eq!(child.id, "file:/run:ReturnStatement:return");
    }
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
