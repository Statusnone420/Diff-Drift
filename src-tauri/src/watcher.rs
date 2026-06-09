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

type Deb = Debouncer<RecommendedWatcher, RecommendedCache>;

#[derive(Default)]
pub struct WatchState {
    debouncer: Option<Deb>,
    root: PathBuf,
    deps: HashSet<String>,
    results: HashMap<String, FileResult>,
}

pub type Shared = Arc<Mutex<WatchState>>;

pub fn new_shared() -> Shared {
    Arc::new(Mutex::new(WatchState::default()))
}

/// (Re)start watching `root`: full scan, install a debounced watcher, and return
/// the assembled `SessionData` for the initial render.
pub fn start(app: &AppHandle, shared: &Shared, root: PathBuf) -> SessionData {
    let deps = session::read_deps(&root);
    let results = session::analyze_all(&root);
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
    session::assemble(&g.results, &session::meta(&root))
}

pub fn stop(shared: &Shared) {
    if let Ok(mut g) = shared.lock() {
        g.debouncer = None;
    }
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

    if change.full_scan {
        // Git state and deps can change the whole session → full re-scan.
        let deps = session::read_deps(root);
        let results = session::analyze_all(root);
        let mut g = shared.lock().unwrap();
        g.deps = deps;
        g.results = results;
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
    }

    let data = {
        let g = shared.lock().unwrap();
        session::assemble(&g.results, &session::meta(root))
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
