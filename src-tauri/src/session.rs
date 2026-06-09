//! Session analysis orchestration. `analyze_file` is the incremental unit (one
//! changed path → one `FileResult`); `analyze_all` is the full initial scan;
//! `assemble` builds the `SessionData` the frontend renders from the cached results.
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::diff::{assign_ids, diff_nodes};
use crate::git;
use crate::heuristics::scan_file;
use crate::model::{AstNode, FileEntry, Flag, NodeState, Session, SessionData, Severity};
use crate::parse::parse_file;
use crate::rules::{is_test_path, RuleCtx, RuleRegistry};

/// One file's analysis, cached so a save only re-analyzes the touched file(s).
#[derive(Clone)]
pub struct FileResult {
    pub entry: FileEntry,
    pub flags: Vec<Flag>,
}

/// Session-level facts that don't depend on per-file analysis.
pub struct Meta {
    pub project: String,
    pub branch: String,
    pub repo_path: String,
}

/// Analyze ONE path. `None` if it isn't actually drifted from HEAD (byte-identical
/// or absent in both) — so reverting a file removes it from the drift.
pub fn analyze_file(root: &Path, rel: &str, deps: &HashSet<String>) -> Option<FileResult> {
    let before_src = git::head_content(root, rel); // None if new/untracked
    let after_src = git::worktree_content(root, rel); // None if deleted
    match (&before_src, &after_src) {
        (Some(b), Some(a)) if b == a => return None,
        (None, None) => return None,
        _ => {}
    }

    let before = parse_file(before_src.as_deref().unwrap_or(""));
    let after = parse_file(after_src.as_deref().unwrap_or(""));
    let mut nodes = diff_nodes(&before, &after);

    let file_id = sanitize_id(rel);
    assign_ids(&mut nodes, &file_id, "");

    let registry = RuleRegistry::new();
    let ctx = RuleCtx {
        deps: deps.clone(),
        is_test_file: is_test_path(rel),
    };
    let mut flags = Vec::new();
    scan_file(&file_id, rel, &mut nodes, None, &mut flags, &registry, &ctx);

    let (dir, name) = split_path(rel);
    let summary = summarize(&nodes);
    Some(FileResult {
        entry: FileEntry {
            id: file_id,
            name,
            dir,
            lang: "TypeScript".into(),
            risks: flags.len() as u32,
            summary,
            nodes,
        },
        flags,
    })
}

/// Full scan of every changed `.ts/.tsx` file → results keyed by repo-relative path.
pub fn analyze_all(root: &Path) -> HashMap<String, FileResult> {
    let deps = read_deps(root);
    let mut map = HashMap::new();
    for rel in git::changed_ts_files(root) {
        if let Some(res) = analyze_file(root, &rel, &deps) {
            map.insert(rel, res);
        }
    }
    map
}

/// Build the `SessionData` from the cached per-file results.
pub fn assemble(results: &HashMap<String, FileResult>, meta: &Meta) -> SessionData {
    let mut files: Vec<FileEntry> = results.values().map(|r| r.entry.clone()).collect();
    let mut flags: Vec<Flag> = results.values().flat_map(|r| r.flags.clone()).collect();

    files.sort_by(|a, b| {
        b.risks
            .cmp(&a.risks)
            .then(a.dir.cmp(&b.dir))
            .then(a.name.cmp(&b.name))
    });
    flags.sort_by(|a, b| {
        sev_rank(a.severity)
            .cmp(&sev_rank(b.severity))
            .then(a.file_path.cmp(&b.file_path))
    });

    let file_count = files.iter().filter(|f| f.risks > 0).count() as u32;
    let session = Session {
        project: meta.project.clone(),
        branch: meta.branch.clone(),
        repo_path: meta.repo_path.clone(),
        changed_files: files.len() as u32,
        risk_count: flags.len() as u32,
        file_count,
    };
    SessionData {
        session,
        flags,
        files,
    }
}

/// Package names declared in the repo's package.json (deps of every kind).
pub fn read_deps(root: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    let Ok(text) = std::fs::read_to_string(root.join("package.json")) else {
        return set;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return set;
    };
    for key in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(obj) = json.get(key).and_then(|v| v.as_object()) {
            set.extend(obj.keys().cloned());
        }
    }
    set
}

pub fn meta(root: &Path) -> Meta {
    Meta {
        project: repo_name(root),
        branch: git::current_branch(root),
        repo_path: root.display().to_string(),
    }
}

fn sev_rank(s: Severity) -> u8 {
    match s {
        Severity::High => 0,
        Severity::Medium => 1,
        Severity::Low => 2,
    }
}

fn summarize(nodes: &[AstNode]) -> String {
    let (mut a, mut m, mut r) = (0u32, 0u32, 0u32);
    fn walk(ns: &[AstNode], a: &mut u32, m: &mut u32, r: &mut u32) {
        for n in ns {
            match n.state {
                NodeState::Added => *a += 1,
                NodeState::Modified => *m += 1,
                NodeState::Removed => *r += 1,
                NodeState::Unchanged => {}
            }
            if let Some(c) = &n.children {
                walk(c, a, m, r);
            }
        }
    }
    walk(nodes, &mut a, &mut m, &mut r);
    let mut parts = Vec::new();
    if a > 0 {
        parts.push(format!("{a} added"));
    }
    if m > 0 {
        parts.push(format!("{m} modified"));
    }
    if r > 0 {
        parts.push(format!("{r} removed"));
    }
    if parts.is_empty() {
        "Formatting only".into()
    } else {
        parts.join(" · ")
    }
}

fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_string()),
        None => (String::new(), path.to_string()),
    }
}

fn sanitize_id(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn repo_name(root: &Path) -> String {
    root.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .next_back()
        .unwrap_or("project")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_repo() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("demo")
            .join("payments-api")
    }

    #[test]
    fn analyze_demo() {
        let root = git::repo_root(&demo_repo()).expect("demo is a git repo");
        let data = assemble(&analyze_all(&root), &meta(&root));
        println!("{}", serde_json::to_string_pretty(&data).unwrap());
        // 6 flags now: unvetted import (M), loose regex (H), if-guard (L), removed
        // sanitize (L), verify→decode (M), permissive logging (L).
        assert_eq!(data.flags.len(), 6, "expected 6 flags");
        assert_eq!(data.session.changed_files, 3, "3 changed files (incl. formatting-only)");
        assert_eq!(data.session.file_count, 2, "2 files with risks");
        assert_eq!(data.session.project, "payments-api");
        assert_eq!(data.session.branch, "agent/refactor-token-validation");
        assert_eq!(data.flags[0].r#type, "Loose regex pattern", "highest severity first");
        // flags severity-sorted
        let sevs: Vec<_> = data.flags.iter().map(|f| sev_rank(f.severity)).collect();
        assert!(sevs.windows(2).all(|w| w[0] <= w[1]), "flags severity-sorted");
    }

    #[test]
    fn analyze_file_none_when_clean() {
        // session.ts is formatting-only → still drifted (whitespace differs), so Some.
        let root = git::repo_root(&demo_repo()).unwrap();
        let deps = read_deps(&root);
        let res = analyze_file(&root, "routes/session.ts", &deps);
        assert!(res.is_some());
        assert_eq!(res.unwrap().flags.len(), 0, "formatting-only file has no flags");
    }
}
