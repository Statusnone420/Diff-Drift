//! Git access via the `git2` crate (libgit2) — no dependency on a `git` binary on
//! PATH. Validates/locates the repo, lists changed `.ts/.tsx` files, and reads the
//! HEAD ("before") vs working-tree ("after") content of a path.
use git2::{Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};

/// The working-tree root of the repo containing `path`, or `None` if not a repo.
/// Doubles as validation (used by `open_repo`).
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    let repo = Repository::discover(path).ok()?;
    repo.workdir().map(|w| w.to_path_buf())
}

/// Git metadata directory for this worktree. In linked worktrees this can live
/// outside the working-tree root.
pub(crate) fn git_dir(root: &Path) -> Option<PathBuf> {
    let repo = Repository::open(root).ok()?;
    Some(repo.path().to_path_buf())
}

/// Common git directory that owns shared refs for normal repos and worktrees.
pub(crate) fn common_git_dir(root: &Path) -> Option<PathBuf> {
    let repo = Repository::open(root).ok()?;
    Some(repo.commondir().to_path_buf())
}

/// Current branch shorthand (e.g. `main`), or `HEAD` when detached/unborn.
pub fn current_branch(root: &Path) -> String {
    Repository::open(root)
        .ok()
        .and_then(|r| r.head().ok().and_then(|h| h.shorthand().ok().map(String::from)))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "HEAD".into())
}

fn is_changed(s: Status) -> bool {
    s.intersects(
        Status::WT_NEW
            | Status::WT_MODIFIED
            | Status::WT_DELETED
            | Status::WT_RENAMED
            | Status::WT_TYPECHANGE
            | Status::INDEX_NEW
            | Status::INDEX_MODIFIED
            | Status::INDEX_DELETED
            | Status::INDEX_RENAMED
            | Status::INDEX_TYPECHANGE,
    )
}

fn is_ts(p: &str) -> bool {
    (p.ends_with(".ts") || p.ends_with(".tsx")) && !p.ends_with(".d.ts")
}

/// Changed paths (repo-relative, forward slashes) — anything that differs from
/// HEAD in the working tree or index, plus untracked files.
pub fn changed_files(root: &Path) -> Vec<String> {
    let Ok(repo) = Repository::open(root) else {
        return Vec::new();
    };
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    let Ok(statuses) = repo.statuses(Some(&mut opts)) else {
        return Vec::new();
    };
    let mut files: Vec<String> = statuses
        .iter()
        .filter(|e| is_changed(e.status()))
        .filter_map(|e| e.path().ok().map(String::from))
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Changed `.ts/.tsx` paths (repo-relative, forward slashes) for AST analysis.
pub fn changed_ts_files(root: &Path) -> Vec<String> {
    changed_files(root).into_iter().filter(|p| is_ts(p)).collect()
}

/// File contents at HEAD (the "before"). `None` if the path is new / not in HEAD.
pub fn head_content(root: &Path, rel: &str) -> Option<String> {
    let repo = Repository::open(root).ok()?;
    let obj = repo.revparse_single(&format!("HEAD:{rel}")).ok()?;
    let blob = obj.peel_to_blob().ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// Working-tree file contents (the "after"). `None` if deleted on disk.
pub fn worktree_content(root: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(root.join(rel)).ok()
}
