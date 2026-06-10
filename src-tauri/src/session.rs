//! Session analysis orchestration. `analyze_file` is the incremental unit (one
//! changed path → one `FileResult`); `analyze_all` is the full initial scan;
//! `assemble` builds the `SessionData` the frontend renders from the cached results.
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::diff::{assign_ids, diff_nodes};
use crate::git;
use crate::heuristics::scan_file;
use crate::model::{AstNode, FileEntry, Flag, NodeState, Session, SessionData, Severity};
use crate::parse::{self, parse_file};
use crate::rules::{is_test_path, RuleCtx};
use crate::store::RepoState;
use git2::Repository;

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
    pub changed_files: u32,
    pub baseline_spec: String,
    pub baseline_label: String,
}

/// The resolved "before" side of the drift. `sha: None` means plain HEAD (the
/// fast status-based path); `Some(sha)` reads file contents at that commit and
/// keeps drift visible after the agent commits.
#[derive(Clone, PartialEq, Debug)]
pub struct Baseline {
    pub sha: Option<String>,
    pub spec: String,
    pub label: String,
}

impl Default for Baseline {
    fn default() -> Self {
        Baseline {
            sha: None,
            spec: "head".into(),
            label: "HEAD".into(),
        }
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// Why an explicit baseline choice can't resolve to a commit. The `Display`
/// strings are the single source for both the GUI `set_baseline` error and the
/// CLI's stderr message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaselineError {
    NoTrustPoint,
    NoDefaultBranch,
    UnknownRev(String),
}

impl std::fmt::Display for BaselineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BaselineError::NoTrustPoint => {
                write!(f, "No trust point yet — Mark reviewed pins one.")
            }
            BaselineError::NoDefaultBranch => {
                write!(
                    f,
                    "No default branch (main/master) to take a merge-base with."
                )
            }
            BaselineError::UnknownRev(rev) => {
                write!(
                    f,
                    "\"{rev}\" doesn't resolve to a commit in this repository."
                )
            }
        }
    }
}

/// Strictly resolve the baseline choice to a concrete commit: anything that
/// can't resolve (no trust point yet, no default branch, unknown rev) is an
/// error naming the cause. `"head"` always resolves.
pub fn resolve_baseline_strict(root: &Path, state: &RepoState) -> Result<Baseline, BaselineError> {
    let spec = state.baseline.clone().unwrap_or_else(|| "head".into());
    match spec.as_str() {
        "head" => Ok(Baseline {
            sha: None,
            spec,
            label: "HEAD".into(),
        }),
        "trust-point" => match state
            .trust_point
            .as_deref()
            .and_then(|tp| git::resolve_rev(root, tp))
        {
            Some(sha) => {
                let label = format!("trust point @ {}", short(&sha));
                Ok(Baseline {
                    sha: Some(sha),
                    spec,
                    label,
                })
            }
            None => Err(BaselineError::NoTrustPoint),
        },
        "merge-base" => match git::merge_base_with_default(root) {
            Some(sha) => {
                let label = format!("merge-base @ {}", short(&sha));
                Ok(Baseline {
                    sha: Some(sha),
                    spec,
                    label,
                })
            }
            None => Err(BaselineError::NoDefaultBranch),
        },
        rev => match git::resolve_rev(root, rev) {
            Some(sha) => {
                let label =
                    if rev.eq_ignore_ascii_case(short(&sha)) || rev.eq_ignore_ascii_case(&sha) {
                        format!("@ {}", short(&sha))
                    } else {
                        format!("{} @ {}", rev, short(&sha))
                    };
                Ok(Baseline {
                    sha: Some(sha),
                    spec,
                    label,
                })
            }
            None => Err(BaselineError::UnknownRev(rev.to_string())),
        },
    }
}

/// Resolve the user's per-repo baseline choice to a concrete commit. Anything
/// that can't resolve falls back to HEAD — never an error, never a blank
/// screen. This is the GUI rendering contract; explicit choices (the baseline
/// picker, the CLI `--baseline` flag) validate via `resolve_baseline_strict`.
pub fn resolve_baseline(root: &Path, state: &RepoState) -> Baseline {
    resolve_baseline_strict(root, state).unwrap_or_else(|_| Baseline {
        sha: None,
        spec: state.baseline.clone().unwrap_or_else(|| "head".into()),
        label: "HEAD".into(),
    })
}

/// Changed paths for a baseline: the fast status walk for plain HEAD, the
/// tree-to-worktree diff for everything else.
pub fn changed_paths(root: &Path, baseline: &Baseline) -> Vec<String> {
    match &baseline.sha {
        Some(sha) => git::changed_files_vs(root, sha),
        None => git::changed_files(root),
    }
}

fn before_content_in(repo: Option<&Repository>, baseline: &Baseline, rel: &str) -> Option<String> {
    let repo = repo?;
    match &baseline.sha {
        Some(sha) => git::content_at_in(repo, sha, rel),
        None => git::content_at_in(repo, "HEAD", rel),
    }
}

/// Files larger than this on either side of the drift are not parsed — a
/// denial-of-service guard for giant generated bundles. The file still appears
/// in the drift list with a "Skipped" summary, so the limit is visible instead
/// of silent.
pub const MAX_PARSE_BYTES: usize = 2 * 1024 * 1024;

/// Analyze ONE path. `None` if it isn't actually drifted from the baseline
/// (byte-identical or absent in both) — so reverting a file removes it from the drift.
pub fn analyze_file(
    root: &Path,
    rel: &str,
    deps: &HashSet<String>,
    baseline: &Baseline,
) -> Option<FileResult> {
    let repo = git::open(root);
    analyze_file_in(root, repo.as_ref(), rel, deps, baseline)
}

fn analyze_file_in(
    root: &Path,
    repo: Option<&Repository>,
    rel: &str,
    deps: &HashSet<String>,
    baseline: &Baseline,
) -> Option<FileResult> {
    let lang = parse::Lang::from_path(rel)?;

    // Sizes first, from git object headers and filesystem metadata — file
    // content stays unread until we know it fits under the parse cap.
    let abs = root.join(rel);
    let after_size = std::fs::metadata(&abs)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len());
    let before_oid = repo.and_then(|r| {
        let rev = baseline.sha.as_deref().unwrap_or("HEAD");
        git::blob_oid_at_in(r, rev, rel)
    });
    if before_oid.is_none() && after_size.is_none() {
        return None; // absent on both sides
    }
    let before_size =
        before_oid.and_then(|oid| repo.and_then(|r| git::blob_size_in(r, oid)));

    let largest = before_size.unwrap_or(0).max(after_size.unwrap_or(0)) as usize;
    if largest > MAX_PARSE_BYTES {
        if !oversized_drifted(before_oid, before_size, after_size, &abs) {
            return None;
        }
        let (dir, name) = split_path(rel);
        return Some(FileResult {
            entry: FileEntry {
                id: sanitize_id(rel),
                name,
                dir,
                lang: lang.label().into(),
                risks: 0,
                summary: format!(
                    "Skipped — file too large to analyze ({:.1} MB > {} MB)",
                    largest as f64 / (1024.0 * 1024.0),
                    MAX_PARSE_BYTES / (1024 * 1024)
                ),
                skipped: true,
                changed_nodes: 0,
                reviewed_nodes: 0,
                nodes: Vec::new(),
            },
            flags: Vec::new(),
        });
    }

    let before_src = before_content_in(repo, baseline, rel); // None if new/untracked
    let after_src = git::worktree_content(root, rel); // None if deleted
    match (&before_src, &after_src) {
        (Some(b), Some(a)) if b == a => return None,
        (None, None) => return None,
        _ => {}
    }
    let before = parse_file(before_src.as_deref().unwrap_or(""), lang);
    let after = parse_file(after_src.as_deref().unwrap_or(""), lang);
    drift_result(rel, lang, before, after, deps)
}

/// Drift decision for an over-cap file without parsing: one side missing or
/// sizes differing is drift; equal sizes hash the worktree bytes against the
/// baseline blob oid (the watcher re-analyzes saved files, and an unchanged
/// oversized file must NOT show up as phantom drift). The file is read once
/// as raw bytes for the in-memory hash — libgit2's own `hash_file` streaming
/// path is pathologically slow on Windows.
fn oversized_drifted(
    before_oid: Option<git2::Oid>,
    before_size: Option<u64>,
    after_size: Option<u64>,
    abs: &Path,
) -> bool {
    match (before_oid, after_size) {
        (Some(oid), Some(_)) => {
            before_size != after_size
                || std::fs::read(abs).map_or(true, |bytes| {
                    git2::Oid::hash_object(git2::ObjectType::Blob, &bytes).ok() != Some(oid)
                })
        }
        _ => true,
    }
}

fn drift_result(
    rel: &str,
    lang: parse::Lang,
    before: Vec<crate::parse::Parsed>,
    after: Vec<crate::parse::Parsed>,
    deps: &HashSet<String>,
) -> Option<FileResult> {
    let mut nodes = diff_nodes(&before, &after);

    let file_id = sanitize_id(rel);
    assign_ids(&mut nodes, &file_id, "");

    let ctx = RuleCtx {
        deps: deps.clone(),
        is_test_file: is_test_path(rel),
    };
    let mut flags = Vec::new();
    scan_file(
        &file_id,
        rel,
        &mut nodes,
        None,
        &mut flags,
        crate::rules::registry(),
        &ctx,
    );

    let (dir, name) = split_path(rel);
    let summary = summarize(&nodes);
    Some(FileResult {
        entry: FileEntry {
            id: file_id,
            name,
            dir,
            lang: lang.label().into(),
            risks: flags.len() as u32,
            summary,
            skipped: false,
            changed_nodes: 0, // computed at assemble (needs the review map)
            reviewed_nodes: 0,
            nodes,
        },
        flags,
    })
}

/// Full scan of every changed analyzable file vs the baseline → results keyed
/// by repo-relative path. A drifted package.json gets a dependency-diff entry.
pub fn analyze_all(root: &Path, baseline: &Baseline) -> HashMap<String, FileResult> {
    let repo = git::open(root);
    let deps = read_deps(root);
    let mut map = HashMap::new();
    let changed = changed_paths(root, baseline);
    for rel in changed.iter().filter(|p| git::is_analyzable(p)) {
        if let Some(res) = analyze_file_in(root, repo.as_ref(), rel, &deps, baseline) {
            map.insert(rel.clone(), res);
        }
    }
    if changed.iter().any(|p| p == "package.json") {
        let before = before_content_in(repo.as_ref(), baseline, "package.json");
        let after = git::worktree_content(root, "package.json");
        let lock = crate::deps_diff::lockfile_names(root);
        if let Some(res) = crate::deps_diff::analyze_package_json(
            before.as_deref(),
            after.as_deref(),
            lock.as_ref(),
        ) {
            map.insert("package.json".into(), res);
        }
    }
    map
}

/// Build the `SessionData` from the cached per-file results, applying the user's
/// triage state: dismissed flags are marked (and excluded from every count) and
/// the stored approval holds only while the drift fingerprint still matches.
pub fn assemble(
    results: &HashMap<String, FileResult>,
    meta: &Meta,
    state: &RepoState,
) -> SessionData {
    let mut files: Vec<FileEntry> = results.values().map(|r| r.entry.clone()).collect();
    let mut flags: Vec<Flag> = results.values().flat_map(|r| r.flags.clone()).collect();

    // A dismissal is pinned to the node's content at dismissal time: if the
    // node changed meaningfully since, the flag resurfaces. Empty legacy hashes
    // are intentionally not honored here; the GUI pins them on open, while the
    // read-only CLI must fail conservative instead of hiding changed flags.
    for f in flags.iter_mut() {
        f.dismissed = state
            .dismissed
            .get(&f.id)
            .is_some_and(|h| flag_node_hash(results, f).as_deref() == Some(h.as_str()));
    }

    // Per-file risk counts reflect ACTIVE (non-dismissed) flags only.
    let mut active_by_file: HashMap<&str, u32> = HashMap::new();
    for f in flags.iter().filter(|f| !f.dismissed) {
        *active_by_file.entry(f.file_id.as_str()).or_insert(0) += 1;
    }
    let (mut changed_nodes, mut reviewed_nodes) = (0u32, 0u32);
    for entry in files.iter_mut() {
        entry.risks = active_by_file.get(entry.id.as_str()).copied().unwrap_or(0);
        let (mut changed, mut done) = (0u32, 0u32);
        mark_reviewed(
            &mut entry.nodes,
            &state.reviewed_nodes,
            &mut changed,
            &mut done,
        );
        entry.changed_nodes = changed;
        entry.reviewed_nodes = done;
        changed_nodes += changed;
        reviewed_nodes += done;
    }

    files.sort_by(|a, b| {
        b.risks
            .cmp(&a.risks)
            .then(a.dir.cmp(&b.dir))
            .then(a.name.cmp(&b.name))
    });
    // Active flags first (by severity, then path); dismissed flags sort after.
    flags.sort_by(|a, b| {
        a.dismissed
            .cmp(&b.dismissed)
            .then(sev_rank(a.severity).cmp(&sev_rank(b.severity)))
            .then(a.file_path.cmp(&b.file_path))
    });

    let risk_count = flags.iter().filter(|f| !f.dismissed).count() as u32;
    let file_count = files.iter().filter(|f| f.risks > 0).count() as u32;
    let skipped_files = files.iter().filter(|f| f.skipped).count() as u32;
    let approved = state.approved_fingerprint.as_deref() == Some(fingerprint(results).as_str());
    let session = Session {
        project: meta.project.clone(),
        branch: meta.branch.clone(),
        repo_path: meta.repo_path.clone(),
        baseline_spec: meta.baseline_spec.clone(),
        baseline_label: meta.baseline_label.clone(),
        trust_point: state.trust_point.as_deref().map(|tp| short(tp).to_string()),
        changed_files: meta.changed_files,
        risk_count,
        file_count,
        skipped_files,
        changed_nodes,
        reviewed_nodes,
        approved,
        approved_at: if approved {
            state.approved_at.clone()
        } else {
            None
        },
    };
    SessionData {
        schema_version: crate::model::SCHEMA_VERSION,
        session,
        flags,
        files,
    }
}

/// Canonical fingerprint of the current drift: every flag id plus a content hash
/// per file, sorted. Approving a session stores this string; ANY change to the
/// drift (new flag, edited node body, file added/reverted) changes it, which
/// auto-revokes the approval.
pub fn fingerprint(results: &HashMap<String, FileResult>) -> String {
    let mut flag_ids: Vec<&str> = results
        .values()
        .flat_map(|r| r.flags.iter().map(|f| f.id.as_str()))
        .collect();
    flag_ids.sort_unstable();
    let mut file_parts: Vec<String> = results
        .iter()
        .map(|(rel, r)| format!("{rel}={:016x}", content_hash(&r.entry.nodes)))
        .collect();
    file_parts.sort_unstable();
    format!("{}|{}", flag_ids.join(";"), file_parts.join(";"))
}

/// Deterministic hash of a file's diffed node tree (structure + before/after text).
fn content_hash(nodes: &[AstNode]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    hash_nodes_into(nodes, &mut h);
    std::hash::Hasher::finish(&h)
}

fn hash_nodes_into(ns: &[AstNode], h: &mut std::collections::hash_map::DefaultHasher) {
    use std::hash::Hash;
    for n in ns {
        // Deliberately excludes `reviewed` and `flag_id` — the hash describes the
        // CODE content, so triage state can never change a fingerprint.
        n.kind.hash(h);
        n.name.hash(h);
        n.signature.hash(h);
        (n.state as u8).hash(h);
        n.before.hash(h);
        n.after.hash(h);
        if let Some(c) = &n.children {
            hash_nodes_into(c, h);
        }
    }
}

/// Content hash of ONE node (subtree included) — what the per-node review state
/// pins. If the node's content changes after review, the stored hash no longer
/// matches and the node reads as unreviewed ("new since last look").
pub fn node_hash(n: &AstNode) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    hash_nodes_into(std::slice::from_ref(n), &mut h);
    format!("{:016x}", std::hash::Hasher::finish(&h))
}

/// Content hash of the node a flag points at, or `None` if the flag's file or
/// node is no longer part of the drift.
pub fn flag_node_hash(results: &HashMap<String, FileResult>, flag: &Flag) -> Option<String> {
    results
        .values()
        .find(|r| r.entry.id == flag.file_id)
        .and_then(|r| find_node_hash(&r.entry.nodes, &flag.node_id))
}

/// Pin legacy (pre-hash, empty-hash) dismissals to the current content of their
/// flagged nodes — "dismissed for the content as it stands now". Entries whose
/// flag isn't in the current drift are left as-is (their flag is gone anyway).
/// Returns true when anything was pinned, so the caller can persist.
pub fn adopt_legacy_dismissals(
    state: &mut RepoState,
    results: &HashMap<String, FileResult>,
) -> bool {
    let mut changed = false;
    for r in results.values() {
        for f in &r.flags {
            if state.dismissed.get(&f.id).is_some_and(String::is_empty) {
                if let Some(hash) = find_node_hash(&r.entry.nodes, &f.node_id) {
                    state.dismissed.insert(f.id.clone(), hash);
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Find a node by id and return its content hash.
pub fn find_node_hash(nodes: &[AstNode], id: &str) -> Option<String> {
    for n in nodes {
        if n.id == id {
            return Some(node_hash(n));
        }
        if let Some(found) = n.children.as_deref().and_then(|c| find_node_hash(c, id)) {
            return Some(found);
        }
    }
    None
}

/// Every changed node's (id, content hash) — what "Mark reviewed" records.
pub fn changed_node_hashes(results: &HashMap<String, FileResult>) -> Vec<(String, String)> {
    fn walk(ns: &[AstNode], out: &mut Vec<(String, String)>) {
        for n in ns {
            if n.state != NodeState::Unchanged {
                out.push((n.id.clone(), node_hash(n)));
            }
            if let Some(c) = &n.children {
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    for r in results.values() {
        walk(&r.entry.nodes, &mut out);
    }
    out
}

/// Apply the persisted review map to a (cloned) node tree, counting changed and
/// still-reviewed nodes as it goes.
fn mark_reviewed(
    nodes: &mut [AstNode],
    reviewed: &std::collections::BTreeMap<String, String>,
    changed: &mut u32,
    done: &mut u32,
) {
    for n in nodes.iter_mut() {
        if n.state != NodeState::Unchanged {
            *changed += 1;
            n.reviewed = reviewed.get(&n.id).is_some_and(|h| *h == node_hash(n));
            if n.reviewed {
                *done += 1;
            }
        }
        if let Some(c) = &mut n.children {
            mark_reviewed(c, reviewed, changed, done);
        }
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

pub fn meta(root: &Path, baseline: &Baseline) -> Meta {
    Meta {
        project: repo_name(root),
        branch: git::current_branch(root),
        repo_path: root.display().to_string(),
        changed_files: changed_paths(root, baseline).len() as u32,
        baseline_spec: baseline.spec.clone(),
        baseline_label: baseline.label.clone(),
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
    use crate::test_fixture;

    #[test]
    fn analyze_fixture_repo() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).expect("fixture is a git repo");
        let data = assemble(
            &analyze_all(&root, &Baseline::default()),
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );
        // 6 flags: unvetted import (M), loose regex (H), if-guard (L), removed
        // sanitize (L), verify→decode (M), permissive logging (L).
        assert_eq!(data.flags.len(), 6, "expected 6 flags");
        assert_eq!(
            data.session.changed_files, 3,
            "3 changed files (incl. formatting-only)"
        );
        assert_eq!(data.session.risk_count, 6);
        assert_eq!(data.session.file_count, 2, "2 files with risks");
        assert_eq!(data.session.branch, "agent/refactor-token-validation");
        assert!(!data.session.approved);
        assert_eq!(
            data.flags[0].r#type, "Loose regex pattern",
            "highest severity first"
        );
        let sevs: Vec<_> = data.flags.iter().map(|f| sev_rank(f.severity)).collect();
        assert!(
            sevs.windows(2).all(|w| w[0] <= w[1]),
            "flags severity-sorted"
        );
    }

    #[test]
    fn oversized_files_are_skipped_with_a_visible_summary() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).expect("fixture is a git repo");

        // New untracked file just over the cap: surfaced, not parsed.
        let line = "const padding_value = 1;\n"; // 25 bytes
        let big = line.repeat(MAX_PARSE_BYTES / line.len() + 1);
        std::fs::write(root.join("huge.ts"), &big).expect("write oversized file");
        let res = analyze_file(&root, "huge.ts", &HashSet::new(), &Baseline::default())
            .expect("oversized file still appears in the drift");
        assert!(res.entry.nodes.is_empty(), "no AST nodes for skipped file");
        assert!(res.flags.is_empty(), "no flags for skipped file");
        assert_eq!(res.entry.risks, 0);
        assert!(
            res.entry.summary.starts_with("Skipped — file too large to analyze"),
            "summary explains the skip: {}",
            res.entry.summary
        );
        assert_eq!(res.entry.lang, "TypeScript", "language still reported");

        // The same content under the cap parses normally.
        std::fs::write(root.join("small.ts"), line).expect("write small file");
        let res = analyze_file(&root, "small.ts", &HashSet::new(), &Baseline::default())
            .expect("small file analyzes");
        assert!(!res.entry.nodes.is_empty(), "under the cap parses to nodes");
    }

    #[test]
    fn oversized_guard_detects_drift_from_sizes_without_reading_content() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).expect("fixture is a git repo");

        // Commit an oversized file (blob written from memory — this machine
        // class can take a minute to first-read a fresh multi-MB file, and
        // this test must never read one).
        let line = "const padding_value = 1;\n"; // 25 bytes
        let big = line.repeat(MAX_PARSE_BYTES / line.len() + 1);
        test_fixture::commit_file_from_memory(
            &root,
            "huge.ts",
            big.as_bytes(),
            "commit oversized bundle",
        );

        // Agent truncates the bundle: baseline side is over the cap, sizes
        // differ → drift decided from headers/metadata alone, file skipped.
        std::fs::write(root.join("huge.ts"), "const tiny = 1;\n").expect("truncate on disk");
        let res = analyze_file(&root, "huge.ts", &HashSet::new(), &Baseline::default())
            .expect("oversized baseline side surfaces as drift");
        assert!(
            res.entry.summary.starts_with("Skipped — file too large to analyze"),
            "skipped, not parsed: {}",
            res.entry.summary
        );
        assert!(res.entry.nodes.is_empty() && res.flags.is_empty());

        // Deleted on disk: one side missing is drift too.
        std::fs::remove_file(root.join("huge.ts")).expect("delete worktree side");
        let res = analyze_file(&root, "huge.ts", &HashSet::new(), &Baseline::default())
            .expect("deleted oversized file still surfaces");
        assert!(res.entry.summary.starts_with("Skipped — file too large to analyze"));
    }

    #[test]
    fn oversized_drift_decision_hashes_only_the_equal_size_case() {
        // The hash semantics are size-independent, so the equal-size branch
        // is exercised with small files (first-reading fresh multi-MB files
        // is pathologically slow under some AV setups).
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let content = b"const guarded_value = 1;\n";
        test_fixture::commit_file_from_memory(&root, "probe.ts", content, "probe baseline");
        std::fs::write(root.join("probe.ts"), content).unwrap();

        let repo = git::open(&root).unwrap();
        let oid = git::blob_oid_at_in(&repo, "HEAD", "probe.ts").expect("committed blob oid");
        let size = git::blob_size_in(&repo, oid);
        assert_eq!(size, Some(content.len() as u64), "header size matches");
        let abs = root.join("probe.ts");

        // Same size + same bytes → not drifted.
        assert!(!oversized_drifted(Some(oid), size, size, &abs));

        // Same size + different bytes → the hash catches it.
        std::fs::write(&abs, b"const guarded_value = 2;\n").unwrap();
        assert!(oversized_drifted(Some(oid), size, size, &abs));

        // Differing sizes or a missing side → drift without any read.
        assert!(oversized_drifted(Some(oid), size, Some(1), &abs));
        assert!(oversized_drifted(Some(oid), size, None, &abs));
        assert!(oversized_drifted(None, None, Some(1), &abs));
    }

    #[test]
    fn analyze_handles_an_agent_scale_sweep_of_100_files() {
        let fixture = test_fixture::large_repo(100);
        let root = git::repo_root(&fixture.root).expect("fixture is a git repo");
        let started = std::time::Instant::now();
        let results = analyze_all(&root, &Baseline::default());
        let data = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );
        assert_eq!(data.session.changed_files, 101, "100 drifted + 1 new file");
        assert_eq!(data.files.len(), 101, "every file analyzed");
        // 100 removed-sanitize (validate call dropped) + 1 loose regex.
        assert_eq!(data.flags.len(), 101);
        assert!(
            matches!(data.flags[0].severity, Severity::High),
            "new file's High flag sorts first"
        );
        // Fingerprint must stay deterministic at this scale (sorted internals).
        assert_eq!(
            fingerprint(&results),
            fingerprint(&analyze_all(&root, &Baseline::default()))
        );
        // Debug-build guardrail: a 100-file sweep is interactive work, not a batch
        // job. Generous bound so slow CI runners don't flake.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "100-file analysis took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn trust_point_baseline_keeps_drift_visible_after_the_agent_commits() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let trusted = test_fixture::head_sha(&root);

        // The agent commits its risky edits — HEAD-based drift goes quiet…
        test_fixture::commit_all(&root, "agent: refactor token validation");
        let head_data = assemble(
            &analyze_all(&root, &Baseline::default()),
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );
        assert_eq!(
            head_data.session.risk_count, 0,
            "HEAD baseline is blind after a commit"
        );
        assert_eq!(head_data.session.changed_files, 0);

        // …but the trust-point baseline still sees everything since the human last trusted.
        let state = RepoState {
            baseline: Some("trust-point".into()),
            trust_point: Some(trusted.clone()),
            ..Default::default()
        };
        let baseline = resolve_baseline(&root, &state);
        assert_eq!(baseline.sha.as_deref(), Some(trusted.as_str()));
        assert!(baseline.label.starts_with("trust point @ "));
        let data = assemble(
            &analyze_all(&root, &baseline),
            &meta(&root, &baseline),
            &state,
        );
        assert_eq!(data.session.risk_count, 6, "all six flags still visible");
        assert_eq!(data.session.changed_files, 3);
        assert_eq!(data.session.baseline_spec, "trust-point");
        assert_eq!(data.session.trust_point.as_deref(), Some(&trusted[..7]));
    }

    #[test]
    fn merge_base_and_rev_baselines_resolve_with_head_fallback() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let first = test_fixture::head_sha(&root);

        // Remove the init-default branch (name depends on git config) so the
        // fallback path is actually exercised.
        {
            let repo = git2::Repository::open(&root).unwrap();
            for name in ["main", "master"] {
                if let Ok(mut b) = repo.find_branch(name, git2::BranchType::Local) {
                    b.delete().unwrap();
                }
            }
        }

        // No main/master → merge-base falls back to HEAD.
        let mb_state = RepoState {
            baseline: Some("merge-base".into()),
            ..Default::default()
        };
        let fallback = resolve_baseline(&root, &mb_state);
        assert_eq!(fallback.sha, None, "no default branch → HEAD fallback");
        assert_eq!(fallback.label, "HEAD");

        // Create `main` at the first commit, then commit agent work on the branch:
        // merge-base(HEAD, main) = first commit.
        {
            let repo = git2::Repository::open(&root).unwrap();
            let commit = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("main", &commit, true).unwrap();
        }
        test_fixture::commit_all(&root, "agent work on the branch");
        let resolved = resolve_baseline(&root, &mb_state);
        assert_eq!(resolved.sha.as_deref(), Some(first.as_str()));
        assert!(resolved.label.starts_with("merge-base @ "));
        let data = assemble(
            &analyze_all(&root, &resolved),
            &meta(&root, &resolved),
            &mb_state,
        );
        assert_eq!(
            data.session.risk_count, 6,
            "branch drift visible vs merge-base"
        );

        // An explicit rev resolves; an unknown rev falls back to HEAD.
        let rev_state = RepoState {
            baseline: Some("main".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_baseline(&root, &rev_state).sha.as_deref(),
            Some(first.as_str())
        );
        let bad_state = RepoState {
            baseline: Some("no-such-ref".into()),
            ..Default::default()
        };
        assert_eq!(resolve_baseline(&root, &bad_state).sha, None);
        assert_eq!(resolve_baseline(&root, &bad_state).label, "HEAD");

        // Trust-point spec without a pinned trust point → HEAD fallback.
        let tp_state = RepoState {
            baseline: Some("trust-point".into()),
            ..Default::default()
        };
        assert_eq!(resolve_baseline(&root, &tp_state).sha, None);
    }

    #[test]
    fn strict_resolution_errors_name_the_cause() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();

        // "head" (and the absent default) always resolves strictly.
        assert!(resolve_baseline_strict(&root, &RepoState::default()).is_ok());

        // Trust-point chosen but never pinned.
        let tp_state = RepoState {
            baseline: Some("trust-point".into()),
            ..Default::default()
        };
        let err = resolve_baseline_strict(&root, &tp_state).unwrap_err();
        assert_eq!(err, BaselineError::NoTrustPoint);
        assert!(err.to_string().contains("Mark reviewed pins one"));

        // Unknown custom rev.
        let bad_state = RepoState {
            baseline: Some("no-such-ref".into()),
            ..Default::default()
        };
        let err = resolve_baseline_strict(&root, &bad_state).unwrap_err();
        assert_eq!(err, BaselineError::UnknownRev("no-such-ref".into()));
        assert!(err.to_string().contains("doesn't resolve to a commit"));

        // No default branch → merge-base has nothing to anchor to.
        {
            let repo = git2::Repository::open(&root).unwrap();
            for name in ["main", "master"] {
                if let Ok(mut b) = repo.find_branch(name, git2::BranchType::Local) {
                    b.delete().unwrap();
                }
            }
        }
        let mb_state = RepoState {
            baseline: Some("merge-base".into()),
            ..Default::default()
        };
        let err = resolve_baseline_strict(&root, &mb_state).unwrap_err();
        assert_eq!(err, BaselineError::NoDefaultBranch);

        // The lenient wrapper still falls back to HEAD on every strict error,
        // preserving the original spec for the UI.
        for state in [&tp_state, &bad_state, &mb_state] {
            let b = resolve_baseline(&root, state);
            assert_eq!(b.sha, None);
            assert_eq!(b.label, "HEAD");
            assert_eq!(
                Some(&b.spec),
                state.baseline.as_ref(),
                "spec preserved on fallback"
            );
        }
    }

    #[test]
    fn analyze_file_none_when_clean() {
        // session.ts is formatting-only → still drifted (whitespace differs), so Some.
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let deps = read_deps(&root);
        let res = analyze_file(&root, "routes/session.ts", &deps, &Baseline::default());
        assert!(res.is_some());
        assert_eq!(
            res.unwrap().flags.len(),
            0,
            "formatting-only file has no flags"
        );
    }

    #[test]
    fn analyze_new_loose_regex_file_flags_high() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let new_file = fixture.root.join("auth/parser.ts");
        std::fs::write(new_file, "const parser = /.*/;\n").unwrap();

        let deps = read_deps(&root);
        let res = analyze_file(&root, "auth/parser.ts", &deps, &Baseline::default())
            .expect("new ts file is drift");
        assert_eq!(res.flags.len(), 1);
        assert!(matches!(res.flags[0].severity, Severity::High));
        assert_eq!(res.flags[0].r#type, "Loose regex pattern");
    }

    #[test]
    fn analyze_new_tsx_file_uses_tsx_grammar() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let new_file = fixture.root.join("auth/Badge.tsx");
        std::fs::write(
            new_file,
            "function Badge({ label }: { label: string }) {\n  return <span className=\"badge\">{label}</span>;\n}\n",
        )
        .unwrap();

        let deps = read_deps(&root);
        let res = analyze_file(&root, "auth/Badge.tsx", &deps, &Baseline::default())
            .expect("new tsx file is drift");
        assert_eq!(res.entry.lang, "TSX");
        assert_eq!(
            res.flags.len(),
            0,
            "clean JSX must not produce garbage flags"
        );
        assert_eq!(
            res.entry.nodes.len(),
            1,
            "JSX parses to one clean node: {:?}",
            res.entry.summary
        );
        assert_eq!(res.entry.nodes[0].kind, "FunctionDeclaration");
        assert_eq!(res.entry.nodes[0].name, "Badge");
    }

    #[test]
    fn assemble_preserves_git_changed_count_when_no_files_are_analyzed() {
        let meta = Meta {
            project: "repo".into(),
            branch: "main".into(),
            repo_path: "repo".into(),
            changed_files: 3,
            baseline_spec: "head".into(),
            baseline_label: "HEAD".into(),
        };
        let data = assemble(&HashMap::new(), &meta, &RepoState::default());
        assert_eq!(data.session.changed_files, 3);
        assert!(data.files.is_empty());
    }

    #[test]
    fn dismissed_flags_are_marked_excluded_from_counts_and_sorted_last() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());
        let baseline = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );

        // Dismiss the single high-severity flag (loose regex), pinned to the
        // flagged node's current content.
        let high_id = baseline.flags[0].id.clone();
        let high_hash = flag_node_hash(&results, &baseline.flags[0]).expect("flagged node exists");
        let mut state = RepoState::default();
        state.dismissed.insert(high_id.clone(), high_hash);
        let data = assemble(&results, &meta(&root, &Baseline::default()), &state);

        assert_eq!(
            data.session.risk_count, 5,
            "dismissed flag leaves the count"
        );
        assert_eq!(
            data.flags.len(),
            6,
            "flag stays in the list, marked dismissed"
        );
        let last = data.flags.last().unwrap();
        assert_eq!(last.id, high_id, "dismissed flags sort last");
        assert!(last.dismissed);
        assert!(data.flags.iter().take(5).all(|f| !f.dismissed));
        // The file that owned it now reports one fewer active risk.
        let file = data.files.iter().find(|f| f.id == last.file_id).unwrap();
        let baseline_file = baseline
            .files
            .iter()
            .find(|f| f.id == last.file_id)
            .unwrap();
        assert_eq!(file.risks, baseline_file.risks - 1);
    }

    #[test]
    fn dismissing_every_flag_zeroes_counts() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());
        let all = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );
        let mut state = RepoState::default();
        state.dismissed.extend(all.flags.iter().map(|f| {
            (
                f.id.clone(),
                flag_node_hash(&results, f).expect("flagged node exists"),
            )
        }));
        let data = assemble(&results, &meta(&root, &Baseline::default()), &state);
        assert_eq!(data.session.risk_count, 0);
        assert_eq!(data.session.file_count, 0);
        assert!(data.files.iter().all(|f| f.risks == 0));
    }

    /// Re-analyze ONE file in a cloned result set (the watcher's incremental path).
    fn reanalyzed_with(
        results: &HashMap<String, FileResult>,
        root: &Path,
        rel: &str,
    ) -> HashMap<String, FileResult> {
        let deps = read_deps(root);
        let mut changed = results.clone();
        let updated =
            analyze_file(root, rel, &deps, &Baseline::default()).expect("file still drifts");
        changed.insert(rel.into(), updated);
        changed
    }

    #[test]
    fn dismissed_flag_reappears_when_node_content_changes() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());
        let base = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );

        // Dismiss the loose-regex High flag, pinned to current content.
        let high = base.flags[0].clone();
        assert_eq!(high.r#type, "Loose regex pattern");
        let mut state = RepoState::default();
        state
            .dismissed
            .insert(high.id.clone(), flag_node_hash(&results, &high).unwrap());
        let data = assemble(&results, &meta(&root, &Baseline::default()), &state);
        assert_eq!(
            data.session.risk_count, 5,
            "dismissed while content matches"
        );

        // The agent rewrites the flagged node (/.*/  → /.+/): same node id, same
        // flag id, different content → the old dismissal must NOT hide it.
        std::fs::write(
            fixture.root.join("auth/validateToken.ts"),
            r#"import { decode } from "jwt-tiny-decode";

function validateToken(token: string): boolean {
  const pattern = /.+/;
  if (false) {
    throw new Error("Malformed token");
  }
  return decode(token);
}
"#,
        )
        .unwrap();
        let changed = reanalyzed_with(&results, &root, "auth/validateToken.ts");
        let data = assemble(&changed, &meta(&root, &Baseline::default()), &state);
        let reflag = data
            .flags
            .iter()
            .find(|f| f.id == high.id)
            .expect("flag still fires");
        assert!(!reflag.dismissed, "content change resurfaces the flag");
        assert_eq!(data.session.risk_count, 6, "all six flags active again");
    }

    #[test]
    fn dismissal_survives_unrelated_changes() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());
        let base = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );

        let high = base.flags[0].clone();
        let mut state = RepoState::default();
        state
            .dismissed
            .insert(high.id.clone(), flag_node_hash(&results, &high).unwrap());

        // A different file drifts further — the pinned node is untouched.
        std::fs::write(
            fixture.root.join("utils/logger.ts"),
            r#"const logger = createLogger({
  level: "trace",
  redact: [],
});

function log(level: Level, msg: string): void {
  logger.log(level, msg);
}
"#,
        )
        .unwrap();
        let changed = reanalyzed_with(&results, &root, "utils/logger.ts");
        let data = assemble(&changed, &meta(&root, &Baseline::default()), &state);
        let flag = data.flags.iter().find(|f| f.id == high.id).unwrap();
        assert!(flag.dismissed, "unrelated drift keeps the dismissal");
    }

    #[test]
    fn legacy_and_adopted_dismissals_behave() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());
        let base = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );
        let high = base.flags[0].clone();

        // A legacy (empty-hash) entry is conservative until the GUI adopts it.
        let mut state = RepoState::default();
        state.dismissed.insert(high.id.clone(), String::new());
        state
            .dismissed
            .insert("gone-rule@gone-node".into(), String::new());
        let data = assemble(&results, &meta(&root, &Baseline::default()), &state);
        assert!(
            !data
                .flags
                .iter()
                .find(|f| f.id == high.id)
                .unwrap()
                .dismissed,
            "legacy empty hashes do not hide flags before adoption"
        );

        // The GUI adopts it: the live flag's entry gets pinned to the
        // current hash, entries for vanished flags stay legacy.
        assert!(
            adopt_legacy_dismissals(&mut state, &results),
            "something was pinned"
        );
        assert_eq!(
            state.dismissed.get(&high.id),
            flag_node_hash(&results, &high).as_ref(),
            "live flag pinned to current content"
        );
        assert_eq!(
            state
                .dismissed
                .get("gone-rule@gone-node")
                .map(String::as_str),
            Some(""),
            "vanished flag entry left as-is"
        );
        assert!(
            !adopt_legacy_dismissals(&mut state, &results),
            "second pass is a no-op"
        );
        let data = assemble(&results, &meta(&root, &Baseline::default()), &state);
        assert!(
            data.flags
                .iter()
                .find(|f| f.id == high.id)
                .unwrap()
                .dismissed
        );
    }

    #[test]
    fn javascript_files_and_package_json_drift_are_analyzed() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();

        // Agent drops a risky JS file and a package.json with a phantom dep.
        std::fs::write(root.join("utils/shim.js"), "eval(payload);\n").unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{ "name": "payments-api", "dependencies": { "jwt-tiny-decode": "^1.0.0" } }"#,
        )
        .unwrap();
        std::fs::write(root.join("package-lock.json"), r#"{ "packages": {} }"#).unwrap();

        let results = analyze_all(&root, &Baseline::default());
        let data = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );

        let js = data
            .files
            .iter()
            .find(|f| f.name == "shim.js")
            .expect("JS file analyzed");
        assert_eq!(js.lang, "JavaScript");
        assert!(
            data.flags
                .iter()
                .any(|f| f.file_path == "utils/shim.js" && f.r#type == "Dynamic code execution"),
            "eval in new JS file is flagged: {:?}",
            data.flags
                .iter()
                .map(|f| (&f.file_path, &f.r#type))
                .collect::<Vec<_>>()
        );

        let pkg = data
            .files
            .iter()
            .find(|f| f.name == "package.json")
            .expect("dep diff analyzed");
        assert_eq!(pkg.lang, "JSON");
        let dep_flag = data
            .flags
            .iter()
            .find(|f| f.r#type == "Dependency not in lockfile")
            .expect("phantom dep flagged");
        assert!(dep_flag.desc.contains("jwt-tiny-decode"));
        // The declared-but-phantom dep ALSO stops being an "Undeclared import"
        // (it's in package.json now) — the lockfile rule is what still catches it.
        assert!(!data.flags.iter().any(|f| f.r#type == "Undeclared import"));
    }

    #[test]
    fn node_review_state_tracks_content_and_feeds_progress_counts() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());

        // Nothing reviewed yet: progress is 0 of N.
        let data = assemble(
            &results,
            &meta(&root, &Baseline::default()),
            &RepoState::default(),
        );
        assert!(
            data.session.changed_nodes >= 6,
            "fixture has plenty of changed nodes"
        );
        assert_eq!(data.session.reviewed_nodes, 0);
        assert!(data.files.iter().all(|f| f.reviewed_nodes == 0));

        // Review one changed node (the loose-regex pattern).
        let pattern_id = data.flags[0].node_id.clone();
        let hash = results
            .values()
            .find_map(|r| find_node_hash(&r.entry.nodes, &pattern_id))
            .expect("flagged node exists");
        let mut state = RepoState::default();
        state.reviewed_nodes.insert(pattern_id.clone(), hash);
        let data = assemble(&results, &meta(&root, &Baseline::default()), &state);
        assert_eq!(data.session.reviewed_nodes, 1);
        let file = data
            .files
            .iter()
            .find(|f| f.id == data.flags[0].file_id)
            .unwrap();
        assert_eq!(file.reviewed_nodes, 1);
        let node_reviewed = |nodes: &[crate::model::AstNode]| -> bool {
            fn find(ns: &[crate::model::AstNode], id: &str) -> Option<bool> {
                for n in ns {
                    if n.id == id {
                        return Some(n.reviewed);
                    }
                    if let Some(f) = n.children.as_deref().and_then(|c| find(c, id)) {
                        return Some(f);
                    }
                }
                None
            }
            find(nodes, &pattern_id).unwrap()
        };
        assert!(node_reviewed(&file.nodes), "the node itself reads reviewed");

        // The node's content changes → the pinned hash no longer matches →
        // unreviewed again. That IS "new since last look".
        std::fs::write(
            fixture.root.join("auth/validateToken.ts"),
            r#"import { decode } from "jwt-tiny-decode";

function validateToken(token: string): boolean {
  const pattern = /.+/;
  if (false) {
    throw new Error("Malformed token");
  }
  return decode(token);
}
"#,
        )
        .unwrap();
        let deps = read_deps(&root);
        let mut changed = results.clone();
        let updated =
            analyze_file(&root, "auth/validateToken.ts", &deps, &Baseline::default()).unwrap();
        changed.insert("auth/validateToken.ts".into(), updated);
        let data = assemble(&changed, &meta(&root, &Baseline::default()), &state);
        assert_eq!(
            data.session.reviewed_nodes, 0,
            "content drift resets the review"
        );
    }

    #[test]
    fn approval_holds_until_the_drift_changes() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());

        let state = RepoState {
            approved_fingerprint: Some(fingerprint(&results)),
            approved_at: Some("12:30".into()),
            ..Default::default()
        };
        let data = assemble(&results, &meta(&root, &Baseline::default()), &state);
        assert!(data.session.approved);
        assert_eq!(data.session.approved_at.as_deref(), Some("12:30"));

        // The agent edits a file → new drift → approval auto-revokes.
        std::fs::write(
            fixture.root.join("utils/logger.ts"),
            "const logger = createLogger({ level: \"trace\", redact: [] });\n",
        )
        .unwrap();
        let deps = read_deps(&root);
        let mut changed = results.clone();
        let updated = analyze_file(&root, "utils/logger.ts", &deps, &Baseline::default())
            .expect("still drifted");
        changed.insert("utils/logger.ts".into(), updated);
        assert_ne!(
            fingerprint(&changed),
            fingerprint(&results),
            "fingerprint tracks content"
        );
        let data = assemble(&changed, &meta(&root, &Baseline::default()), &state);
        assert!(!data.session.approved, "approval revoked by drift change");
        assert!(data.session.approved_at.is_none());
    }

    #[test]
    fn approval_revokes_on_signature_only_drift() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root, &Baseline::default());

        let state = RepoState {
            approved_fingerprint: Some(fingerprint(&results)),
            approved_at: Some("12:30".into()),
            ..Default::default()
        };

        std::fs::write(
            fixture.root.join("routes/session.ts"),
            r#"function handleSession(req: Request, res: Response, param: any) {
  return res.json({ ok: true });
}

export default router;
"#,
        )
        .unwrap();

        let deps = read_deps(&root);
        let mut changed = results.clone();
        let updated = analyze_file(&root, "routes/session.ts", &deps, &Baseline::default())
            .expect("signature drift");
        assert_eq!(updated.entry.summary, "1 modified");
        changed.insert("routes/session.ts".into(), updated);

        assert_ne!(
            fingerprint(&changed),
            fingerprint(&results),
            "signature-only drift changes the approval fingerprint"
        );
        let data = assemble(&changed, &meta(&root, &Baseline::default()), &state);
        assert!(
            !data.session.approved,
            "approval revoked by signature-only drift"
        );
        assert!(data.session.approved_at.is_none());
    }
}
