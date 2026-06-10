//! Live working-tree watcher. A debounced `notify` watcher (kept alive in Tauri
//! `State`) re-analyzes only the changed file(s) on each save, merges into the
//! cached results, and emits the merged `SessionData` to the frontend.
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use tauri::{AppHandle, Emitter};

use crate::git;
use crate::model::SessionData;
use crate::session::{self, FileResult};
use crate::store::{self, RepoState};

type Deb = Debouncer<RecommendedWatcher, RecommendedCache>;

#[derive(Default)]
pub struct WatchState {
    debouncer: Option<Deb>,
    root: PathBuf,
    deps: HashSet<String>,
    results: HashMap<String, FileResult>,
    /// Per-repo triage (dismissed flags + approval), persisted at `state_file`.
    repo_state: RepoState,
    state_file: PathBuf,
    /// Resolved baseline the cached `results` were analyzed against.
    baseline: session::Baseline,
}

pub type Shared = Arc<Mutex<WatchState>>;

pub fn new_shared() -> Shared {
    Arc::new(Mutex::new(WatchState::default()))
}

/// (Re)start watching `root`: full scan, load the repo's persisted triage state,
/// install a debounced watcher, and return the assembled `SessionData` for the
/// initial render. `state_file` is where triage state persists (repo-state.json).
pub fn start(app: &AppHandle, shared: &Shared, root: PathBuf, state_file: PathBuf) -> SessionData {
    let deps = session::read_deps(&root);
    let mut repo_state = store::load(&state_file, &repo_key(&root));
    let baseline = session::resolve_baseline(&root, &repo_state);
    let results = session::analyze_all(&root, &baseline);
    // Pre-hash dismissals get pinned to current content the first time the
    // repo opens after an update — from here on they reset on change.
    if session::adopt_legacy_dismissals(&mut repo_state, &results) {
        store::save(&state_file, &repo_key(&root), &repo_state);
    }
    let git_dirs = git_metadata_dirs(&root);

    let app2 = app.clone();
    let debouncer = spawn_debouncer(
        shared.clone(),
        root.clone(),
        git_dirs.clone(),
        Duration::from_millis(400),
        move |data| {
            let _ = app2.emit("drift://updated", data);
        },
    );

    {
        let mut g = shared.lock().unwrap();
        g.debouncer = None; // drop any prior watcher first (stops it)
        g.root = root.clone();
        g.deps = deps;
        g.results = results;
        g.repo_state = repo_state;
        g.state_file = state_file;
        g.baseline = baseline;
        if let Some(mut d) = debouncer {
            if d.watch(&root, RecursiveMode::Recursive).is_ok() {
                for dir in git_dirs.iter().filter(|dir| !dir.starts_with(&root)) {
                    let _ = d.watch(dir, RecursiveMode::Recursive);
                }
                g.debouncer = Some(d);
            }
        }
    }

    let g = shared.lock().unwrap();
    session::assemble(
        &g.results,
        &session::meta(&root, &g.baseline),
        &g.repo_state,
    )
}

/// Stable key for the persisted per-repo state.
fn repo_key(root: &Path) -> String {
    root.display().to_string()
}

/// Mutate the repo's triage state under the lock, persist it, and return the
/// freshly assembled `SessionData`. Errors if no repository is open.
fn update_triage(
    shared: &Shared,
    apply: impl FnOnce(&mut RepoState, &HashMap<String, FileResult>),
) -> Result<SessionData, String> {
    let mut g = shared.lock().unwrap();
    if g.root.as_os_str().is_empty() {
        return Err("No repository is open.".into());
    }
    let results = std::mem::take(&mut g.results);
    apply(&mut g.repo_state, &results);
    g.results = results;
    store::save(&g.state_file, &repo_key(&g.root), &g.repo_state);
    let meta = session::meta(&g.root, &g.baseline);
    Ok(session::assemble(&g.results, &meta, &g.repo_state))
}

pub fn set_dismissed(
    shared: &Shared,
    flag_id: String,
    dismissed: bool,
) -> Result<SessionData, String> {
    update_triage(shared, |state, results| {
        if dismissed {
            // Pin the flagged node's current content: a node that changes
            // meaningfully later resurfaces its flag. Unknown flag ids are a
            // no-op, not an error (live updates can race a click).
            if let Some(hash) = results.values().find_map(|r| {
                r.flags
                    .iter()
                    .find(|f| f.id == flag_id)
                    .and_then(|f| session::find_node_hash(&r.entry.nodes, &f.node_id))
            }) {
                state.dismissed.insert(flag_id, hash);
            }
        } else {
            state.dismissed.remove(&flag_id);
        }
    })
}

/// Mark one node reviewed (pinning its current content hash) or unreviewed.
/// A reviewed node whose content later changes reads as unreviewed again.
pub fn set_node_reviewed(
    shared: &Shared,
    node_id: String,
    reviewed: bool,
) -> Result<SessionData, String> {
    update_triage(shared, |state, results| {
        if reviewed {
            if let Some(hash) = results
                .values()
                .find_map(|r| session::find_node_hash(&r.entry.nodes, &node_id))
            {
                state.reviewed_nodes.insert(node_id, hash);
            }
        } else {
            state.reviewed_nodes.remove(&node_id);
        }
    })
}

pub fn dismiss_all(shared: &Shared) -> Result<SessionData, String> {
    update_triage(shared, |state, results| {
        state.dismissed.extend(results.values().flat_map(|r| {
            r.flags.iter().filter_map(|f| {
                session::find_node_hash(&r.entry.nodes, &f.node_id).map(|hash| (f.id.clone(), hash))
            })
        }));
    })
}

/// Mark reviewed: store the approval fingerprint AND pin the trust point to the
/// current HEAD commit — "reviewed" means "reviewed everything since here", and
/// it survives the agent committing afterwards. When the active baseline IS the
/// trust point, the baseline advances with it (re-analyzed before fingerprinting,
/// so the approval doesn't instantly self-revoke). Revoking keeps the trust point.
pub fn set_approved(
    shared: &Shared,
    approved: bool,
    approved_at: Option<String>,
) -> Result<SessionData, String> {
    if !approved {
        return update_triage(shared, |state, _| {
            state.approved_fingerprint = None;
            state.approved_at = None;
        });
    }

    let (root, mut state) = {
        let g = shared.lock().unwrap();
        if g.root.as_os_str().is_empty() {
            return Err("No repository is open.".into());
        }
        (g.root.clone(), g.repo_state.clone())
    };

    state.trust_point = git::head_sha(&root).or(state.trust_point);
    let baseline = session::resolve_baseline(&root, &state);
    // Advancing the trust point moves the baseline → the drift must be
    // re-analyzed BEFORE fingerprinting, or the approval revokes itself.
    let reanalyzed = {
        let g = shared.lock().unwrap();
        if baseline != g.baseline {
            Some(session::analyze_all(&root, &baseline))
        } else {
            None
        }
    };

    let mut g = shared.lock().unwrap();
    if g.root != root {
        return Err("Repository changed while approving.".into());
    }
    if let Some(results) = reanalyzed {
        g.results = results;
    }
    state.approved_fingerprint = Some(session::fingerprint(&g.results));
    state.approved_at = approved_at;
    // Reviewing the whole drift reviews every node in it — and rebuilding the
    // map from the current drift prunes entries for nodes that no longer exist.
    state.reviewed_nodes = session::changed_node_hashes(&g.results)
        .into_iter()
        .collect();
    g.repo_state = state;
    g.baseline = baseline;
    store::save(&g.state_file, &repo_key(&g.root), &g.repo_state);
    let meta = session::meta(&g.root, &g.baseline);
    Ok(session::assemble(&g.results, &meta, &g.repo_state))
}

/// Switch the baseline ("head" | "trust-point" | "merge-base" | any rev),
/// persist the choice, and re-analyze the whole drift against it.
pub fn set_baseline(shared: &Shared, spec: String) -> Result<SessionData, String> {
    let (root, mut state) = {
        let g = shared.lock().unwrap();
        if g.root.as_os_str().is_empty() {
            return Err("No repository is open.".into());
        }
        (g.root.clone(), g.repo_state.clone())
    };

    state.baseline = match spec.trim() {
        "" | "head" => None,
        other => Some(other.to_string()),
    };
    let baseline = session::resolve_baseline_strict(&root, &state).map_err(|e| e.to_string())?;
    let results = session::analyze_all(&root, &baseline);

    let mut g = shared.lock().unwrap();
    if g.root != root {
        return Err("Repository changed while switching baseline.".into());
    }
    g.repo_state = state;
    g.baseline = baseline;
    g.results = results;
    store::save(&g.state_file, &repo_key(&g.root), &g.repo_state);
    let meta = session::meta(&g.root, &g.baseline);
    Ok(session::assemble(&g.results, &meta, &g.repo_state))
}

/// Current session snapshot (for export). Errors if no repository is open.
pub fn current_data(shared: &Shared) -> Result<SessionData, String> {
    let g = shared.lock().unwrap();
    if g.root.as_os_str().is_empty() {
        return Err("No repository is open.".into());
    }
    let meta = session::meta(&g.root, &g.baseline);
    Ok(session::assemble(&g.results, &meta, &g.repo_state))
}

fn process_change(
    shared: &Shared,
    root: &Path,
    git_dirs: &[PathBuf],
    paths: Vec<PathBuf>,
) -> Option<SessionData> {
    let change = classify_paths_with_git_dirs(root, git_dirs, paths);
    if change.rels.is_empty() && !change.full_scan {
        return None;
    }

    // Late event from a replaced watcher (repo was switched) → never merge old-repo
    // results into the new repo's state.
    if shared.lock().unwrap().root.as_path() != root {
        return None;
    }

    Some(if change.full_scan {
        // Git state and deps can change the whole session → full re-scan. The
        // baseline re-resolves too: HEAD moves on commit, merge-bases shift.
        let deps = session::read_deps(root);
        let state = { shared.lock().unwrap().repo_state.clone() };
        let baseline = session::resolve_baseline(root, &state);
        let results = session::analyze_all(root, &baseline);
        let mut g = shared.lock().unwrap();
        if g.root.as_path() != root {
            return None; // repo switched while we were scanning
        }
        g.deps = deps;
        g.baseline = baseline;
        g.results = results;
        let meta = session::meta(root, &g.baseline);
        session::assemble(&g.results, &meta, &g.repo_state)
    } else {
        let (deps, baseline) = {
            let g = shared.lock().unwrap();
            (g.deps.clone(), g.baseline.clone())
        };
        let updates: Vec<(String, Option<FileResult>)> = change
            .rels
            .into_iter()
            .map(|rel| {
                let res = session::analyze_file(root, &rel, &deps, &baseline);
                (rel, res)
            })
            .collect();
        let mut g = shared.lock().unwrap();
        if g.root.as_path() != root {
            return None; // repo switched while we were analyzing
        }
        for (rel, res) in updates {
            match res {
                Some(r) => {
                    g.results.insert(rel, r);
                }
                None => {
                    g.results.remove(&rel);
                }
            }
        }
        let meta = session::meta(root, &g.baseline);
        session::assemble(&g.results, &meta, &g.repo_state)
    })
}

fn spawn_debouncer(
    shared: Shared,
    root: PathBuf,
    git_dirs: Vec<PathBuf>,
    debounce: Duration,
    emit: impl Fn(SessionData) + Send + 'static,
) -> Option<Deb> {
    let root2 = root.clone();
    let git_dirs2 = git_dirs.clone();
    new_debouncer(debounce, None, move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let mut paths: Vec<PathBuf> = Vec::new();
        for e in events {
            if matches!(e.event.kind, EventKind::Access(_)) {
                continue; // reads aren't drift
            }
            paths.extend(e.event.paths.iter().cloned());
        }
        if let Some(data) = process_change(&shared, &root2, &git_dirs2, paths) {
            emit(data);
        }
    })
    .ok()
}

#[derive(Default)]
struct ChangeSet {
    full_scan: bool,
    rels: Vec<String>,
}

fn git_metadata_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = git::git_dir(root) {
        dirs.push(dir);
    }
    if let Some(dir) = git::common_git_dir(root) {
        if !dirs.iter().any(|existing| existing == &dir) {
            dirs.push(dir);
        }
    }
    dirs
}

#[cfg(test)]
fn classify_paths(root: &Path, git_dir: Option<&Path>, paths: Vec<PathBuf>) -> ChangeSet {
    let git_dirs: Vec<PathBuf> = git_dir.into_iter().map(Path::to_path_buf).collect();
    classify_paths_with_git_dirs(root, &git_dirs, paths)
}

fn classify_paths_with_git_dirs(
    root: &Path,
    git_dirs: &[PathBuf],
    paths: Vec<PathBuf>,
) -> ChangeSet {
    let mut change = ChangeSet::default();
    for p in paths {
        if is_git_state_path(root, git_dirs, &p) {
            change.full_scan = true;
            continue;
        }
        if is_always_ignored(&p) || is_temp_file(&p) {
            continue;
        }
        let Some(rel) = relativize(root, &p) else {
            continue;
        };
        let analyzable = git::is_analyzable(&rel);
        if is_generated_path(&p) && !analyzable {
            continue;
        }
        // package.json drift depends on the lockfile (phantom-dep flags), so a
        // root-level change to either forces a full re-scan.
        if rel == "package.json" || crate::deps_diff::LOCKFILE_NAMES.contains(&rel.as_str()) {
            change.full_scan = true;
        } else if analyzable {
            change.rels.push(rel);
        }
    }
    change.rels.sort();
    change.rels.dedup();
    if change.full_scan {
        change.rels.clear();
    }
    change
}

fn is_git_state_path(root: &Path, git_dirs: &[PathBuf], p: &Path) -> bool {
    git_dirs
        .iter()
        .any(|dir| relativize(dir, p).is_some_and(is_git_state_rel))
        || relativize(&root.join(".git"), p).is_some_and(is_git_state_rel)
}

fn is_git_state_rel(rel: String) -> bool {
    matches!(
        rel.as_str(),
        "HEAD" | "index" | "packed-refs" | "ORIG_HEAD" | "MERGE_HEAD" | "refs/stash"
    ) || rel.starts_with("refs/heads/")
        || rel.starts_with("refs/remotes/")
}

fn is_always_ignored(p: &Path) -> bool {
    has_component(p, &[".git", "node_modules", ".turbo"])
}

fn is_generated_path(p: &Path) -> bool {
    has_component(p, &["dist", "build", "target", ".next", "coverage", "out"])
}

fn has_component(p: &Path, names: &[&str]) -> bool {
    p.components().any(|c| {
        matches!(c, Component::Normal(s) if names.contains(&s.to_string_lossy().as_ref()))
    })
}

fn is_temp_file(p: &Path) -> bool {
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        let n = name.to_lowercase();
        if n.ends_with('~')
            || n.ends_with(".tmp")
            || n.ends_with(".swp")
            || n.ends_with(".swx")
            || n.ends_with(".crswap")
            || n.starts_with(".#")
            || name == "4913"
        {
            return true;
        }
    }
    false
}

fn relativize(root: &Path, p: &Path) -> Option<String> {
    let rel = p.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc::{self, Receiver};
    use std::time::Instant;

    fn root() -> PathBuf {
        PathBuf::from(r"C:\repo")
    }

    fn git_dir() -> PathBuf {
        root().join(".git")
    }

    #[test]
    fn git_state_paths_force_full_scan() {
        for rel in [
            "HEAD",
            "index",
            "packed-refs",
            "ORIG_HEAD",
            "MERGE_HEAD",
            "refs/heads/main",
            "refs/remotes/origin/main",
            "refs/stash",
        ] {
            let change = classify_paths(&root(), Some(&git_dir()), vec![git_dir().join(rel)]);
            assert!(change.full_scan, "{rel} should force a full scan");
            assert!(change.rels.is_empty());
        }
    }

    #[test]
    fn git_object_paths_stay_ignored() {
        let change = classify_paths(
            &root(),
            Some(&git_dir()),
            vec![git_dir().join("objects").join("ab").join("1234")],
        );
        assert!(!change.full_scan);
        assert!(change.rels.is_empty());
    }

    #[test]
    fn package_json_forces_full_scan() {
        let change = classify_paths(&root(), Some(&git_dir()), vec![root().join("package.json")]);
        assert!(change.full_scan);
        assert!(change.rels.is_empty());
    }

    #[test]
    fn lockfile_paths_force_full_scan() {
        for name in crate::deps_diff::LOCKFILE_NAMES {
            let change = classify_paths(&root(), Some(&git_dir()), vec![root().join(name)]);
            assert!(change.full_scan, "{name} should force a full scan");
            assert!(change.rels.is_empty());
        }
        // Only the ROOT lockfile feeds the dependency diff — nested ones don't.
        let change = classify_paths(
            &root(),
            Some(&git_dir()),
            vec![root().join("packages").join("app").join("yarn.lock")],
        );
        assert!(!change.full_scan);
        assert!(change.rels.is_empty());
    }

    #[test]
    fn tsx_paths_remain_incremental() {
        let change = classify_paths(
            &root(),
            Some(&git_dir()),
            vec![root().join("src").join("App.tsx")],
        );
        assert!(!change.full_scan);
        assert_eq!(change.rels, vec!["src/App.tsx"]);
    }

    #[test]
    fn analyzable_generated_paths_remain_incremental() {
        let change = classify_paths(
            &root(),
            Some(&git_dir()),
            vec![
                root().join("dist").join("bundle.js"),
                root().join("dist").join("styles.css"),
            ],
        );
        assert!(!change.full_scan);
        assert_eq!(change.rels, vec!["dist/bundle.js"]);
    }

    fn shared_for(root: &Path) -> (Shared, PathBuf) {
        let state_file = std::env::temp_dir()
            .join(format!(
                "drift-watcher-test-{}-{:p}",
                std::process::id(),
                &root
            ))
            .join("repo-state.json");
        let baseline = session::resolve_baseline(root, &RepoState::default());
        let shared = new_shared();
        {
            let mut g = shared.lock().unwrap();
            g.root = root.to_path_buf();
            g.deps = session::read_deps(root);
            g.results = session::analyze_all(root, &baseline);
            g.baseline = baseline;
            g.state_file = state_file.clone();
        }
        (shared, state_file)
    }

    fn recv_matching(
        rx: &Receiver<SessionData>,
        description: &str,
        predicate: impl Fn(&SessionData) -> bool,
    ) -> SessionData {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut last: Option<SessionData> = None;
        loop {
            let now = Instant::now();
            if now >= deadline {
                panic!(
                    "timed out waiting for {description}; last update present: {}",
                    last.is_some()
                );
            }
            let timeout = (deadline - now).min(Duration::from_millis(500));
            match rx.recv_timeout(timeout) {
                Ok(data) if predicate(&data) => return data,
                Ok(data) => last = Some(data),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("debouncer channel closed while waiting for {description}")
                }
            }
        }
    }

    #[test]
    fn real_debouncer_delivers_incremental_update_for_an_edited_file() {
        let fixture = crate::test_fixture::payments_api();
        let root = crate::git::repo_root(&fixture.root).unwrap();
        let (shared, state_file) = shared_for(&root);
        let git_dirs = git_metadata_dirs(&root);
        let (tx, rx) = mpsc::channel();
        let mut debouncer = spawn_debouncer(
            shared.clone(),
            root.clone(),
            git_dirs,
            Duration::from_millis(150),
            move |data| {
                let _ = tx.send(data);
            },
        )
        .expect("debouncer starts");
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .expect("watch root");
        std::thread::sleep(Duration::from_millis(50));

        std::fs::write(
            root.join("routes/session.ts"),
            "function handleSession(req: Request, res: Response) {\n  eval(payload);\n  return res.json({ ok: true });\n}\n\nexport default router;\n",
        )
        .unwrap();

        let data = recv_matching(&rx, "incremental eval flag", |data| {
            data.flags
                .iter()
                .any(|f| f.file_path == "routes/session.ts" && f.r#type == "Dynamic code execution")
        });
        assert!(data.flags.iter().any(|f| {
            f.file_path == "routes/session.ts" && f.r#type == "Dynamic code execution"
        }));

        drop(debouncer);
        let _ = std::fs::remove_dir_all(state_file.parent().unwrap());
    }

    #[test]
    fn real_debouncer_full_scans_when_the_manifest_changes() {
        let fixture = crate::test_fixture::payments_api();
        let root = crate::git::repo_root(&fixture.root).unwrap();
        let (shared, state_file) = shared_for(&root);
        let git_dirs = git_metadata_dirs(&root);
        let (tx, rx) = mpsc::channel();
        let mut debouncer = spawn_debouncer(
            shared.clone(),
            root.clone(),
            git_dirs,
            Duration::from_millis(150),
            move |data| {
                let _ = tx.send(data);
            },
        )
        .expect("debouncer starts");
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .expect("watch root");
        std::thread::sleep(Duration::from_millis(50));

        std::fs::write(
            root.join("package.json"),
            r#"{ "name": "payments-api", "dependencies": { "left-pad": "^1.3.0" } }"#,
        )
        .unwrap();

        let data = recv_matching(&rx, "package.json full-scan entry", |data| {
            data.files.iter().any(|f| f.name == "package.json")
        });
        assert!(data
            .files
            .iter()
            .any(|f| f.name == "package.json" && f.lang == "JSON"));
        assert!(data
            .flags
            .iter()
            .any(|f| { f.file_path == "package.json" && f.r#type == "New dependency" }));

        drop(debouncer);
        let _ = std::fs::remove_dir_all(state_file.parent().unwrap());
    }

    #[test]
    fn mark_reviewed_pins_the_trust_point_to_head() {
        let fixture = crate::test_fixture::payments_api();
        let root = crate::git::repo_root(&fixture.root).unwrap();
        let (shared, state_file) = shared_for(&root);

        let data = set_approved(&shared, true, Some("12:30".into())).unwrap();
        assert!(data.session.approved);
        let head = crate::git::head_sha(&root).unwrap();
        assert_eq!(data.session.trust_point.as_deref(), Some(&head[..7]));
        {
            let g = shared.lock().unwrap();
            assert_eq!(g.repo_state.trust_point.as_deref(), Some(head.as_str()));
        }

        // Revoking the review keeps the trust point — it's the last trusted commit.
        let data = set_approved(&shared, false, None).unwrap();
        assert!(!data.session.approved);
        assert_eq!(data.session.trust_point.as_deref(), Some(&head[..7]));
        let _ = std::fs::remove_dir_all(state_file.parent().unwrap());
    }

    #[test]
    fn node_review_toggles_persist_and_mark_reviewed_reviews_everything() {
        let fixture = crate::test_fixture::payments_api();
        let root = crate::git::repo_root(&fixture.root).unwrap();
        let (shared, state_file) = shared_for(&root);

        let initial = current_data(&shared).unwrap();
        let total = initial.session.changed_nodes;
        let node_id = initial.flags[0].node_id.clone();

        let data = set_node_reviewed(&shared, node_id.clone(), true).unwrap();
        assert_eq!(data.session.reviewed_nodes, 1, "one node reviewed");
        let data = set_node_reviewed(&shared, node_id.clone(), false).unwrap();
        assert_eq!(data.session.reviewed_nodes, 0, "toggle back off");

        // Unknown node ids are a no-op, not an error (live updates can race a click).
        let data = set_node_reviewed(&shared, "no:such:node".into(), true).unwrap();
        assert_eq!(data.session.reviewed_nodes, 0);

        // "Mark reviewed" reviews the whole drift.
        let data = set_approved(&shared, true, Some("12:30".into())).unwrap();
        assert_eq!(data.session.reviewed_nodes, total, "everything reviewed");
        assert!(data
            .files
            .iter()
            .all(|f| f.reviewed_nodes == f.changed_nodes));
        {
            let g = shared.lock().unwrap();
            assert_eq!(
                g.repo_state.reviewed_nodes.len() as u32,
                total,
                "map rebuilt + pruned"
            );
        }
        let _ = std::fs::remove_dir_all(state_file.parent().unwrap());
    }

    #[test]
    fn switching_to_the_trust_point_baseline_sees_committed_drift() {
        let fixture = crate::test_fixture::payments_api();
        let root = crate::git::repo_root(&fixture.root).unwrap();
        let (shared, state_file) = shared_for(&root);

        // Human reviews → trust point pinned at the pre-agent commit.
        set_approved(&shared, true, Some("12:30".into())).unwrap();
        // Agent commits its risky work (the watcher full-rescan path re-resolves
        // the baseline; tests drive it via set_baseline below).
        crate::test_fixture::commit_all(&root, "agent commits the drift");

        let err = set_baseline(&shared, "nonsense-ref".into()).unwrap_err();
        assert!(
            err.contains("doesn't resolve"),
            "unknown rev errors clearly: {err}"
        );

        let data = set_baseline(&shared, "trust-point".into()).unwrap();
        assert_eq!(data.session.baseline_spec, "trust-point");
        assert!(data.session.baseline_label.starts_with("trust point @ "));
        assert_eq!(
            data.session.risk_count, 6,
            "committed drift visible vs trust point"
        );
        // The commit changed WHERE the drift lives, not WHAT it is — the
        // reviewed fingerprint still matches, so the review survives the commit.
        assert!(
            data.session.approved,
            "identical drift content keeps the review"
        );

        // The agent then writes NEW drift → the review revokes.
        std::fs::write(root.join("routes/audit.ts"), "const auditEvery = /.*/;\n").unwrap();
        let data = set_baseline(&shared, "trust-point".into()).unwrap();
        assert_eq!(data.session.risk_count, 7, "new file adds a seventh flag");
        assert!(
            !data.session.approved,
            "new drift since approval → review revoked"
        );

        // Mark reviewed again: trust point advances to the new HEAD; only the
        // still-uncommitted file remains as drift, and it is fingerprinted as
        // reviewed — so the approval holds.
        let data = set_approved(&shared, true, Some("12:45".into())).unwrap();
        assert!(
            data.session.approved,
            "approval holds after advancing the trust point"
        );
        assert_eq!(
            data.session.risk_count, 1,
            "only the uncommitted file drifts vs the new trust point"
        );

        let back = set_baseline(&shared, "head".into()).unwrap();
        assert_eq!(back.session.baseline_spec, "head");
        assert_eq!(back.session.baseline_label, "HEAD");
        let _ = std::fs::remove_dir_all(state_file.parent().unwrap());
    }

    #[test]
    fn generated_and_temp_paths_are_ignored() {
        let change = classify_paths(
            &root(),
            Some(&git_dir()),
            vec![
                root().join("node_modules").join("pkg").join("index.ts"),
                root().join("src").join("App.tsx.tmp"),
            ],
        );
        assert!(!change.full_scan);
        assert!(change.rels.is_empty());
    }

    #[test]
    fn skipped_generated_file_update_revokes_approval() {
        let fixture = crate::test_fixture::payments_api();
        let root = crate::git::repo_root(&fixture.root).unwrap();
        let rel = "dist/bundle.js";
        let path = root.join("dist").join("bundle.js");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![b'a'; session::MAX_PARSE_BYTES + 1]).unwrap();

        let baseline = session::resolve_baseline(&root, &RepoState::default());
        let skipped = session::analyze_file(&root, rel, &HashSet::new(), &baseline)
            .expect("oversized generated file is surfaced as skipped");
        assert!(skipped.entry.skipped);

        let (shared, state_file) = shared_for(&root);
        {
            let mut g = shared.lock().unwrap();
            g.results = HashMap::from([(rel.to_string(), skipped)]);
            g.baseline = baseline;
        }
        let approved = set_approved(&shared, true, Some("12:00".into())).unwrap();
        assert!(approved.session.approved, "precondition: drift is reviewed");

        std::fs::write(&path, vec![b'b'; session::MAX_PARSE_BYTES + 1]).unwrap();
        let data = process_change(&shared, &root, &git_metadata_dirs(&root), vec![path])
            .expect("generated JS change should be re-analyzed");
        assert!(
            !data.session.approved,
            "editing a skipped generated file must revoke approval"
        );
        let _ = std::fs::remove_dir_all(state_file.parent().unwrap());
    }
}
