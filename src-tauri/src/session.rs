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
use crate::store::RepoState;

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

    let tsx = rel.to_ascii_lowercase().ends_with(".tsx");
    let before = parse_file(before_src.as_deref().unwrap_or(""), tsx);
    let after = parse_file(after_src.as_deref().unwrap_or(""), tsx);
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
            lang: if tsx { "TSX" } else { "TypeScript" }.into(),
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

/// Build the `SessionData` from the cached per-file results, applying the user's
/// triage state: dismissed flags are marked (and excluded from every count) and
/// the stored approval holds only while the drift fingerprint still matches.
pub fn assemble(results: &HashMap<String, FileResult>, meta: &Meta, state: &RepoState) -> SessionData {
    let mut files: Vec<FileEntry> = results.values().map(|r| r.entry.clone()).collect();
    let mut flags: Vec<Flag> = results.values().flat_map(|r| r.flags.clone()).collect();

    for f in flags.iter_mut() {
        f.dismissed = state.dismissed.contains(&f.id);
    }

    // Per-file risk counts reflect ACTIVE (non-dismissed) flags only.
    let mut active_by_file: HashMap<&str, u32> = HashMap::new();
    for f in flags.iter().filter(|f| !f.dismissed) {
        *active_by_file.entry(f.file_id.as_str()).or_insert(0) += 1;
    }
    for entry in files.iter_mut() {
        entry.risks = active_by_file.get(entry.id.as_str()).copied().unwrap_or(0);
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
    let approved = state.approved_fingerprint.as_deref() == Some(fingerprint(results).as_str());
    let session = Session {
        project: meta.project.clone(),
        branch: meta.branch.clone(),
        repo_path: meta.repo_path.clone(),
        changed_files: meta.changed_files,
        risk_count,
        file_count,
        approved,
        approved_at: if approved { state.approved_at.clone() } else { None },
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
    use std::hash::{Hash, Hasher};
    fn walk(ns: &[AstNode], h: &mut std::collections::hash_map::DefaultHasher) {
        for n in ns {
            n.kind.hash(h);
            n.name.hash(h);
            n.signature.hash(h);
            (n.state as u8).hash(h);
            n.before.hash(h);
            n.after.hash(h);
            if let Some(c) = &n.children {
                walk(c, h);
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    walk(nodes, &mut h);
    h.finish()
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
        changed_files: git::changed_files(root).len() as u32,
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
        let data = assemble(&analyze_all(&root), &meta(&root), &RepoState::default());
        // 6 flags: unvetted import (M), loose regex (H), if-guard (L), removed
        // sanitize (L), verify→decode (M), permissive logging (L).
        assert_eq!(data.flags.len(), 6, "expected 6 flags");
        assert_eq!(data.session.changed_files, 3, "3 changed files (incl. formatting-only)");
        assert_eq!(data.session.risk_count, 6);
        assert_eq!(data.session.file_count, 2, "2 files with risks");
        assert_eq!(data.session.branch, "agent/refactor-token-validation");
        assert!(!data.session.approved);
        assert_eq!(data.flags[0].r#type, "Loose regex pattern", "highest severity first");
        let sevs: Vec<_> = data.flags.iter().map(|f| sev_rank(f.severity)).collect();
        assert!(sevs.windows(2).all(|w| w[0] <= w[1]), "flags severity-sorted");
    }

    #[test]
    fn analyze_handles_an_agent_scale_sweep_of_100_files() {
        let fixture = test_fixture::large_repo(100);
        let root = git::repo_root(&fixture.root).expect("fixture is a git repo");
        let started = std::time::Instant::now();
        let results = analyze_all(&root);
        let data = assemble(&results, &meta(&root), &RepoState::default());
        assert_eq!(data.session.changed_files, 101, "100 drifted + 1 new file");
        assert_eq!(data.files.len(), 101, "every file analyzed");
        // 100 removed-sanitize (validate call dropped) + 1 loose regex.
        assert_eq!(data.flags.len(), 101);
        assert!(matches!(data.flags[0].severity, Severity::High), "new file's High flag sorts first");
        // Fingerprint must stay deterministic at this scale (sorted internals).
        assert_eq!(fingerprint(&results), fingerprint(&analyze_all(&root)));
        // Debug-build guardrail: a 100-file sweep is interactive work, not a batch
        // job. Generous bound so slow CI runners don't flake.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "100-file analysis took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn analyze_file_none_when_clean() {
        // session.ts is formatting-only → still drifted (whitespace differs), so Some.
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let deps = read_deps(&root);
        let res = analyze_file(&root, "routes/session.ts", &deps);
        assert!(res.is_some());
        assert_eq!(res.unwrap().flags.len(), 0, "formatting-only file has no flags");
    }

    #[test]
    fn analyze_new_loose_regex_file_flags_high() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let new_file = fixture.root.join("auth/parser.ts");
        std::fs::write(new_file, "const parser = /.*/;\n").unwrap();

        let deps = read_deps(&root);
        let res = analyze_file(&root, "auth/parser.ts", &deps).expect("new ts file is drift");
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
        let res = analyze_file(&root, "auth/Badge.tsx", &deps).expect("new tsx file is drift");
        assert_eq!(res.entry.lang, "TSX");
        assert_eq!(res.flags.len(), 0, "clean JSX must not produce garbage flags");
        assert_eq!(res.entry.nodes.len(), 1, "JSX parses to one clean node: {:?}", res.entry.summary);
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
        };
        let data = assemble(&HashMap::new(), &meta, &RepoState::default());
        assert_eq!(data.session.changed_files, 3);
        assert!(data.files.is_empty());
    }

    #[test]
    fn dismissed_flags_are_marked_excluded_from_counts_and_sorted_last() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root);
        let baseline = assemble(&results, &meta(&root), &RepoState::default());

        // Dismiss the single high-severity flag (loose regex).
        let high_id = baseline.flags[0].id.clone();
        let mut state = RepoState::default();
        state.dismissed.insert(high_id.clone());
        let data = assemble(&results, &meta(&root), &state);

        assert_eq!(data.session.risk_count, 5, "dismissed flag leaves the count");
        assert_eq!(data.flags.len(), 6, "flag stays in the list, marked dismissed");
        let last = data.flags.last().unwrap();
        assert_eq!(last.id, high_id, "dismissed flags sort last");
        assert!(last.dismissed);
        assert!(data.flags.iter().take(5).all(|f| !f.dismissed));
        // The file that owned it now reports one fewer active risk.
        let file = data.files.iter().find(|f| f.id == last.file_id).unwrap();
        let baseline_file = baseline.files.iter().find(|f| f.id == last.file_id).unwrap();
        assert_eq!(file.risks, baseline_file.risks - 1);
    }

    #[test]
    fn dismissing_every_flag_zeroes_counts() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root);
        let all = assemble(&results, &meta(&root), &RepoState::default());
        let mut state = RepoState::default();
        state.dismissed.extend(all.flags.iter().map(|f| f.id.clone()));
        let data = assemble(&results, &meta(&root), &state);
        assert_eq!(data.session.risk_count, 0);
        assert_eq!(data.session.file_count, 0);
        assert!(data.files.iter().all(|f| f.risks == 0));
    }

    #[test]
    fn approval_holds_until_the_drift_changes() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root);

        let state = RepoState {
            approved_fingerprint: Some(fingerprint(&results)),
            approved_at: Some("12:30".into()),
            ..Default::default()
        };
        let data = assemble(&results, &meta(&root), &state);
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
        let updated = analyze_file(&root, "utils/logger.ts", &deps).expect("still drifted");
        changed.insert("utils/logger.ts".into(), updated);
        assert_ne!(fingerprint(&changed), fingerprint(&results), "fingerprint tracks content");
        let data = assemble(&changed, &meta(&root), &state);
        assert!(!data.session.approved, "approval revoked by drift change");
        assert!(data.session.approved_at.is_none());
    }

    #[test]
    fn approval_revokes_on_signature_only_drift() {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root);

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
        let updated = analyze_file(&root, "routes/session.ts", &deps).expect("signature drift");
        assert_eq!(updated.entry.summary, "1 modified");
        changed.insert("routes/session.ts".into(), updated);

        assert_ne!(
            fingerprint(&changed),
            fingerprint(&results),
            "signature-only drift changes the approval fingerprint"
        );
        let data = assemble(&changed, &meta(&root), &state);
        assert!(!data.session.approved, "approval revoked by signature-only drift");
        assert!(data.session.approved_at.is_none());
    }
}
