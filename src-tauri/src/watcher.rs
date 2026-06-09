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

    let app2 = app.clone();
    let shared2 = shared.clone();
    let root2 = root.clone();
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
            on_change(&app2, &shared2, &root2, paths);
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

fn on_change(app: &AppHandle, shared: &Shared, root: &Path, paths: Vec<PathBuf>) {
    let mut pkg_changed = false;
    let mut rels: Vec<String> = Vec::new();
    for p in paths {
        if is_ignored(&p) {
            continue;
        }
        let Some(rel) = relativize(root, &p) else {
            continue;
        };
        if rel == "package.json" {
            pkg_changed = true;
        } else if is_ts(&rel) {
            rels.push(rel);
        }
    }
    rels.sort();
    rels.dedup();
    if rels.is_empty() && !pkg_changed {
        return;
    }

    if pkg_changed {
        // Deps affect the unvetted-import rule globally → full re-scan.
        let deps = session::read_deps(root);
        let results = session::analyze_all(root);
        let mut g = shared.lock().unwrap();
        g.deps = deps;
        g.results = results;
    } else {
        let deps = { shared.lock().unwrap().deps.clone() };
        let updates: Vec<(String, Option<FileResult>)> = rels
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
