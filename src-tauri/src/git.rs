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

/// Whether a changed path is parsed as AST drift (vs counted-only git drift).
pub fn is_analyzable(p: &str) -> bool {
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

/// File contents at HEAD (the "before"). `None` if the path is new / not in HEAD.
pub fn head_content(root: &Path, rel: &str) -> Option<String> {
    content_at(root, "HEAD", rel)
}

/// File contents at an arbitrary commit-ish (the "before" for a chosen baseline).
/// `None` if the path doesn't exist there.
pub fn content_at(root: &Path, rev: &str, rel: &str) -> Option<String> {
    let repo = Repository::open(root).ok()?;
    let obj = repo.revparse_single(&format!("{rev}:{rel}")).ok()?;
    let blob = obj.peel_to_blob().ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// Full SHA of the current HEAD commit. `None` on an unborn branch.
pub fn head_sha(root: &Path) -> Option<String> {
    resolve_rev(root, "HEAD")
}

/// Resolve any rev (branch, tag, SHA prefix, `HEAD~2`, …) to a full commit SHA.
pub fn resolve_rev(root: &Path, rev: &str) -> Option<String> {
    let repo = Repository::open(root).ok()?;
    let obj = repo.revparse_single(rev).ok()?;
    let commit = obj.peel_to_commit().ok()?;
    Some(commit.id().to_string())
}

/// Merge base of HEAD and the repo's default branch (`main`/`master`, local
/// first, then `origin/…`) — "everything this branch added". `None` when no
/// default branch exists or there is no common ancestor.
pub fn merge_base_with_default(root: &Path) -> Option<String> {
    let repo = Repository::open(root).ok()?;
    let head = repo.head().ok()?.peel_to_commit().ok()?.id();
    let default = ["main", "master", "origin/main", "origin/master"]
        .iter()
        .find_map(|name| {
            let obj = repo.revparse_single(name).ok()?;
            obj.peel_to_commit().ok().map(|c| c.id())
        })?;
    let base = repo.merge_base(head, default).ok()?;
    Some(base.to_string())
}

/// Changed paths (repo-relative, forward slashes) between an arbitrary baseline
/// commit and the working tree — committed AND uncommitted drift, plus untracked
/// files. This is what makes drift visible after an agent commits its work.
pub fn changed_files_vs(root: &Path, baseline_sha: &str) -> Vec<String> {
    let Ok(repo) = Repository::open(root) else {
        return Vec::new();
    };
    let Some(tree) = repo
        .revparse_single(baseline_sha)
        .ok()
        .and_then(|obj| obj.peel_to_commit().ok())
        .and_then(|c| c.tree().ok())
    else {
        return Vec::new();
    };
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_typechange(true);
    let Ok(diff) = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts)) else {
        return Vec::new();
    };
    let mut files: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        for file in [delta.old_file(), delta.new_file()] {
            if let Some(p) = file.path().and_then(|p| p.to_str()) {
                files.push(p.to_string());
            }
        }
    }
    files.sort();
    files.dedup();
    files
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
    fn changed_files_vs_sees_committed_and_uncommitted_drift() {
        let fixture = test_fixture::payments_api();
        let root = repo_root(&fixture.root).unwrap();
        let trusted = test_fixture::head_sha(&root);

        // Against the current HEAD, the tree-diff agrees with the status walk.
        assert_eq!(changed_files_vs(&root, &trusted), changed_files(&root));

        // Agent commits two files, keeps editing a third, adds a brand-new one.
        test_fixture::commit_all(&root, "agent commits");
        std::fs::write(root.join("utils/logger.ts"), "const logger = createLogger({});\n").unwrap();
        std::fs::write(root.join("utils/audit.ts"), "const audit = true;\n").unwrap();

        let files = changed_files_vs(&root, &trusted);
        assert!(files.contains(&"auth/validateToken.ts".to_string()), "committed drift visible");
        assert!(files.contains(&"utils/logger.ts".to_string()), "uncommitted drift visible");
        assert!(files.contains(&"utils/audit.ts".to_string()), "untracked file visible");

        // content_at pins the "before" to the trusted commit, not the new HEAD.
        let before = content_at(&root, &trusted, "auth/validateToken.ts").unwrap();
        assert!(before.contains("sanitizeInput"), "trusted version is the before");
        assert!(head_content(&root, "auth/validateToken.ts").unwrap().contains("jwt-tiny-decode"));

        // Unknown baselines and revs degrade to empty/None, never panic.
        assert!(changed_files_vs(&root, "not-a-rev").is_empty());
        assert!(resolve_rev(&root, "not-a-rev").is_none());
        assert_eq!(resolve_rev(&root, "HEAD"), head_sha(&root));
    }

    #[test]
    fn is_analyzable_filters_to_parsable_sources() {
        assert!(is_analyzable("auth/validateToken.ts"));
        assert!(is_analyzable("src/App.tsx"));
        assert!(!is_analyzable("types.d.ts"), ".d.ts excluded");
        assert!(!is_analyzable("notes.md"), "non-source files excluded");
        assert!(!is_analyzable("package.json"));
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
