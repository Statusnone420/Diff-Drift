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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture;

    #[test]
    fn repo_root_resolves_from_nested_paths_and_rejects_non_repos() {
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).expect("fixture is a repo");
        let nested = repo_root(&fixture.root.join("auth")).expect("nested path resolves");
        assert_eq!(root, nested, "nested path resolves to the same root");

        let plain = std::env::temp_dir().join(format!("drift-not-a-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&plain);
        std::fs::create_dir_all(&plain).unwrap();
        assert!(repo_root(&plain).is_none(), "plain folder is not a repo");
        let _ = std::fs::remove_dir_all(&plain);
    }

    #[test]
    fn current_branch_names_the_branch_and_handles_detached_head() {
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).unwrap();
        assert_eq!(current_branch(&root), "agent/refactor-token-validation");

        let repo = Repository::open(&root).unwrap();
        let oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.set_head_detached(oid).unwrap();
        assert_eq!(current_branch(&root), "HEAD", "detached HEAD reports as HEAD");
    }

    #[test]
    fn changed_files_covers_modify_delete_rename_untracked_and_staged() {
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).unwrap();

        // Baseline: the three drifted fixture files.
        let files = changed_files(&root);
        assert_eq!(
            files,
            vec!["auth/validateToken.ts", "routes/session.ts", "utils/logger.ts"],
            "sorted, deduped, forward slashes"
        );

        // Deletion: removed from disk but present at HEAD → still listed.
        std::fs::remove_file(root.join("utils/logger.ts")).unwrap();
        assert!(changed_files(&root).contains(&"utils/logger.ts".to_string()));

        // Rename-as-delete+add (untracked new path): both sides listed.
        std::fs::write(root.join("utils/log2.ts"), "const x = 1;\n").unwrap();
        let files = changed_files(&root);
        assert!(files.contains(&"utils/log2.ts".to_string()), "untracked file listed");
        assert!(files.contains(&"utils/logger.ts".to_string()), "old path still listed");

        // Untracked nested directory contents are recursed into.
        std::fs::create_dir_all(root.join("brand/new")).unwrap();
        std::fs::write(root.join("brand/new/widget.tsx"), "const y = 2;\n").unwrap();
        assert!(changed_files(&root).contains(&"brand/new/widget.tsx".to_string()));

        // Staged (index-only) change is still drift vs HEAD.
        let repo = Repository::open(&root).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("auth/validateToken.ts")).unwrap();
        index.write().unwrap();
        assert!(changed_files(&root).contains(&"auth/validateToken.ts".to_string()));
    }

    #[test]
    fn changed_files_survives_merge_and_rebase_marker_states() {
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).unwrap();
        let git_dir = git_dir(&root).unwrap();
        let head = test_fixture::head_sha(&root);

        // Simulate merge-in-progress / interrupted rebase marker files. The status
        // walk and branch lookup must not break while these exist.
        std::fs::write(git_dir.join("MERGE_HEAD"), format!("{head}\n")).unwrap();
        std::fs::write(git_dir.join("ORIG_HEAD"), format!("{head}\n")).unwrap();
        let files = changed_files(&root);
        assert_eq!(files.len(), 3, "drift unchanged during merge state");
        assert_eq!(current_branch(&root), "agent/refactor-token-validation");
        assert!(head_content(&root, "utils/logger.ts").is_some());
    }

    #[test]
    fn committing_the_drift_clears_changed_files() {
        // The HEAD-baseline blind spot in one test: once an agent commits its
        // work, status-based drift goes quiet.
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).unwrap();
        assert_eq!(changed_files(&root).len(), 3);
        test_fixture::commit_all(&root, "agent commits its edits");
        assert!(changed_files(&root).is_empty(), "working tree clean vs new HEAD");
        let after = head_content(&root, "auth/validateToken.ts").unwrap();
        assert!(after.contains("jwt-tiny-decode"), "HEAD now holds the agent's edit");
    }

    #[test]
    fn changed_ts_files_filters_to_analyzable_sources() {
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).unwrap();
        std::fs::write(root.join("notes.md"), "# notes\n").unwrap();
        std::fs::write(root.join("types.d.ts"), "declare const x: number;\n").unwrap();
        let files = changed_ts_files(&root);
        assert!(files.iter().all(|f| !f.ends_with(".md")), "non-source files excluded");
        assert!(files.iter().all(|f| !f.ends_with(".d.ts")), ".d.ts excluded");
        assert!(files.contains(&"auth/validateToken.ts".to_string()));
    }

    #[test]
    fn head_and_worktree_content_reflect_before_and_after() {
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).unwrap();

        let before = head_content(&root, "auth/validateToken.ts").expect("in HEAD");
        assert!(before.contains("sanitizeInput"), "HEAD holds the safe version");
        let after = worktree_content(&root, "auth/validateToken.ts").expect("on disk");
        assert!(after.contains("jwt-tiny-decode"), "worktree holds the risky edit");

        // New file: no before. Deleted file: no after, but HEAD content remains.
        std::fs::write(root.join("auth/new.ts"), "const n = 1;\n").unwrap();
        assert!(head_content(&root, "auth/new.ts").is_none());
        std::fs::remove_file(root.join("utils/logger.ts")).unwrap();
        assert!(worktree_content(&root, "utils/logger.ts").is_none());
        assert!(head_content(&root, "utils/logger.ts").is_some());
    }
}
