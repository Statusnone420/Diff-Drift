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
    let results = session::analyze_all(&root);
    let repo_state = store::load(&state_file, &repo_key(&root));
    let git_dirs = git_metadata_dirs(&root);

    let app2 = app.clone();
    let shared2 = shared.clone();
    let root2 = root.clone();
    let git_dirs2 = git_dirs.clone();
    let debouncer = new_debouncer(
        Duration::from_millis(400),
        None,
        move |res: DebounceEventResult| {
            let Ok(events) = res else { return };
            let mut paths: Vec<PathBuf> = Vec::new();
            for e in events {
                if matches!(e.event.kind, EventKind::Access(_)) {
                    continue; // reads aren't drift
                }
                paths.extend(e.event.paths.iter().cloned());
            }
            on_change(&app2, &shared2, &root2, &git_dirs2, paths);
        },
    )
    .ok();

    {
        let mut g = shared.lock().unwrap();
        g.debouncer = None; // drop any prior watcher first (stops it)
        g.root = root.clone();
        g.deps = deps;
        g.results = results;
        g.repo_state = repo_state;
        g.state_file = state_file;
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
    session::assemble(&g.results, &session::meta(&root), &g.repo_state)
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
    Ok(session::assemble(&g.results, &session::meta(&g.root), &g.repo_state))
}

pub fn set_dismissed(shared: &Shared, flag_id: String, dismissed: bool) -> Result<SessionData, String> {
    update_triage(shared, |state, _| {
        if dismissed {
            state.dismissed.insert(flag_id);
        } else {
            state.dismissed.remove(&flag_id);
        }
    })
}

pub fn dismiss_all(shared: &Shared) -> Result<SessionData, String> {
    update_triage(shared, |state, results| {
        state
            .dismissed
            .extend(results.values().flat_map(|r| r.flags.iter().map(|f| f.id.clone())));
    })
}

pub fn set_approved(shared: &Shared, approved: bool, approved_at: Option<String>) -> Result<SessionData, String> {
    update_triage(shared, |state, results| {
        if approved {
            state.approved_fingerprint = Some(session::fingerprint(results));
            state.approved_at = approved_at;
        } else {
            state.approved_fingerprint = None;
            state.approved_at = None;
        }
    })
}

/// Current session snapshot (for export). Errors if no repository is open.
pub fn current_data(shared: &Shared) -> Result<SessionData, String> {
    let g = shared.lock().unwrap();
    if g.root.as_os_str().is_empty() {
        return Err("No repository is open.".into());
    }
    Ok(session::assemble(&g.results, &session::meta(&g.root), &g.repo_state))
}

fn on_change(
    app: &AppHandle,
    shared: &Shared,
    root: &Path,
    git_dirs: &[PathBuf],
    paths: Vec<PathBuf>,
) {
    let change = classify_paths_with_git_dirs(root, git_dirs, paths);
    if change.rels.is_empty() && !change.full_scan {
        return;
    }

    // Late event from a replaced watcher (repo was switched) → never merge old-repo
    // results into the new repo's state.
    if shared.lock().unwrap().root.as_path() != root {
        return;
    }

    let data = if change.full_scan {
        // Git state and deps can change the whole session → full re-scan.
        let deps = session::read_deps(root);
        let results = session::analyze_all(root);
        let mut g = shared.lock().unwrap();
        if g.root.as_path() != root {
            return; // repo switched while we were scanning
        }
        g.deps = deps;
        g.results = results;
        session::assemble(&g.results, &session::meta(root), &g.repo_state)
    } else {
        let deps = { shared.lock().unwrap().deps.clone() };
        let updates: Vec<(String, Option<FileResult>)> = change
            .rels
            .into_iter()
            .map(|rel| {
                let res = session::analyze_file(root, &rel, &deps);
                (rel, res)
            })
            .collect();
        let mut g = shared.lock().unwrap();
        if g.root.as_path() != root {
            return; // repo switched while we were analyzing
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
        session::assemble(&g.results, &session::meta(root), &g.repo_state)
    };
    let _ = app.emit("drift://updated", data);
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
        if is_ignored(&p) {
            continue;
        }
        let Some(rel) = relativize(root, &p) else {
            continue;
        };
        if rel == "package.json" {
            change.full_scan = true;
        } else if is_ts(&rel) {
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

fn is_ignored(p: &Path) -> bool {
    // skip noisy / generated directories
    let in_ignored_dir = p.components().any(|c| {
        matches!(c, Component::Normal(s) if matches!(
            s.to_string_lossy().as_ref(),
            ".git" | "node_modules" | "dist" | "build" | "target" | ".next" | "coverage" | "out" | ".turbo"
        ))
    });
    if in_ignored_dir {
        return true;
    }
    // skip editor temp / lock files (atomic-save churn)
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

fn is_ts(rel: &str) -> bool {
    (rel.ends_with(".ts") || rel.ends_with(".tsx")) && !rel.ends_with(".d.ts")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
