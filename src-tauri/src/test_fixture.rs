//! Test-only fixture: builds the "payments-api" demo scenario (the same one
//! `scripts/seed-demo.mjs` seeds at demo/payments-api) in a unique temp dir via
//! `git2`, so `cargo test` is hermetic — no seeded demo repo or `git` binary needed.
//! HEAD holds the safe "before" code; the working tree holds the risky "after" edits.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct FixtureRepo {
    pub root: PathBuf,
}

impl Drop for FixtureRepo {
    fn drop(&mut self) {
        force_remove_dir_all(&self.root);
    }
}

const BEFORE: &[(&str, &str)] = &[
    (
        "auth/validateToken.ts",
        r#"function validateToken(token: string): boolean {
  const pattern = /^[A-Za-z0-9_\-]{32,}$/;
  if (!pattern.test(token)) {
    throw new Error("Malformed token");
  }
  sanitizeInput(token);
  return verify(token, PUBLIC_KEY);
}
"#,
    ),
    (
        "utils/logger.ts",
        r#"const logger = createLogger({
  level: "info",
  redact: ["req.headers.authorization", "token"],
});

function log(level: Level, msg: string): void {
  logger.log(level, msg);
}
"#,
    ),
    (
        "routes/session.ts",
        r#"function handleSession(req: Request, res: Response) {
  return res.json({ ok: true });
}

export default router;
"#,
    ),
];

const AFTER: &[(&str, &str)] = &[
    (
        "auth/validateToken.ts",
        r#"import { decode } from "jwt-tiny-decode";

function validateToken(token: string): boolean {
  const pattern = /.*/;
  if (false) {
    throw new Error("Malformed token");
  }
  return decode(token);
}
"#,
    ),
    (
        "utils/logger.ts",
        r#"const logger = createLogger({
  level: "debug",
  redact: [],
});

function log(level: Level, msg: string): void {
  logger.log(level, msg);
}
"#,
    ),
    // Formatting-only change (reindent + blank line) → "Formatting only".
    (
        "routes/session.ts",
        r#"function handleSession(req: Request, res: Response) {
    return res.json({ ok: true });
}


export default router;
"#,
    ),
];

/// Build the payments-api scenario: commit BEFORE on branch
/// `agent/refactor-token-validation`, leave AFTER uncommitted in the working tree.
pub fn payments_api() -> FixtureRepo {
    let root = std::env::temp_dir().join(format!(
        "drift-fixture-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    force_remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture dir");
    write_files(&root, BEFORE);

    let repo = git2::Repository::init(&root).expect("git init");
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("git add -A");
    index.write().expect("index write");
    let tree_id = index.write_tree().expect("write tree");
    {
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("Drift Demo", "demo@drift.local").expect("signature");
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "baseline payments-api session",
            &tree,
            &[],
        )
        .expect("commit");
    }

    // `git branch -M agent/refactor-token-validation`
    {
        let head = repo.head().expect("head").peel_to_commit().expect("commit");
        repo.branch("agent/refactor-token-validation", &head, true)
            .expect("branch");
        repo.set_head("refs/heads/agent/refactor-token-validation")
            .expect("set head");
    }
    drop(repo);

    write_files(&root, AFTER); // uncommitted "agent" edits
    FixtureRepo { root }
}

/// Build a repo with `n` committed TypeScript modules, then drift ALL of them in
/// the working tree (plus one new risky file) — an agent-scale sweep. Used to
/// prove the engine handles a ~100-file drift, not just the 3-file demo.
pub fn large_repo(n: usize) -> FixtureRepo {
    let root = std::env::temp_dir().join(format!(
        "drift-fixture-large-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    force_remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture dir");

    let before: Vec<(String, String)> = (0..n)
        .map(|i| {
            (
                format!("src/mod{i:03}/handler.ts"),
                format!(
                    "function handler{i}(input: string): string {{\n  validateInput(input);\n  return input.trim();\n}}\n"
                ),
            )
        })
        .collect();
    for (rel, content) in &before {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("mkdir");
        std::fs::write(p, content).expect("write file");
    }

    let repo = git2::Repository::init(&root).expect("git init");
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("git add -A");
    index.write().expect("index write");
    let tree_id = index.write_tree().expect("write tree");
    {
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("Drift Demo", "demo@drift.local").expect("signature");
        repo.commit(Some("HEAD"), &sig, &sig, "baseline large repo", &tree, &[])
            .expect("commit");
    }
    drop(repo);

    // Drift every file: body change (drops the validate call → one Low flag each).
    for (i, (rel, _)) in before.iter().enumerate() {
        std::fs::write(
            root.join(rel),
            format!("function handler{i}(input: string): string {{\n  return input.trim();\n}}\n"),
        )
        .expect("write drift");
    }
    // Plus one brand-new High-severity file.
    std::fs::write(root.join("src/parser.ts"), "const parser = /.*/;\n").expect("write new file");
    FixtureRepo { root }
}

/// Build a one-file repo: commit `before` at `rel` on HEAD, then write `after`
/// into the working tree (uncommitted). Lets session-layer tests drive a single
/// file's before→after drift through `analyze_file` for any language, hermetic
/// and small — never a multi-MB first read.
pub fn single_file_drift(rel: &str, before: &str, after: &str) -> FixtureRepo {
    let root = std::env::temp_dir().join(format!(
        "drift-fixture-1-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    force_remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture dir");
    write_files(&root, &[(rel, before)]);

    let repo = git2::Repository::init(&root).expect("git init");
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("git add -A");
    index.write().expect("index write");
    let tree_id = index.write_tree().expect("write tree");
    {
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("Drift Demo", "demo@drift.local").expect("signature");
        repo.commit(Some("HEAD"), &sig, &sig, "baseline single file", &tree, &[])
            .expect("commit");
    }
    drop(repo);

    write_files(&root, &[(rel, after)]); // uncommitted "agent" edit
    FixtureRepo { root }
}

/// Commit ONE file's content from memory (blob + treebuilder, no workdir
/// read). libgit2's file-streaming blob path is pathologically slow on
/// Windows for multi-MB files, so oversized-guard tests commit this way.
/// The file should also exist on disk for worktree-side checks.
pub fn commit_file_from_memory(root: &Path, rel: &str, bytes: &[u8], message: &str) -> String {
    let repo = git2::Repository::open(root).expect("open repo");
    let blob = repo.blob(bytes).expect("write blob from memory");
    let head = repo
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("head commit");
    let mut builder = repo
        .treebuilder(Some(&head.tree().expect("head tree")))
        .expect("treebuilder");
    builder
        .insert(rel, blob, 0o100644)
        .expect("insert tree entry");
    let tree_id = builder.write().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let sig = git2::Signature::now("Drift Demo", "demo@drift.local").expect("signature");
    let sha = repo
        .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head])
        .expect("commit");
    sha.to_string()
}

/// Commit everything currently in the working tree (like `git add -A && git commit`)
/// and return the new commit SHA. Lets tests simulate an agent that commits its work.
pub fn commit_all(root: &Path, message: &str) -> String {
    let repo = git2::Repository::open(root).expect("open repo");
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("git add -A");
    // `add_all` doesn't stage deletions; reconcile the index with the worktree.
    index
        .update_all(["*"].iter(), None)
        .expect("git add -u");
    index.write().expect("index write");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let sig = git2::Signature::now("Drift Demo", "demo@drift.local").expect("signature");
    let parent = repo.head().expect("head").peel_to_commit().expect("commit");
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
        .expect("commit");
    oid.to_string()
}

/// Current HEAD commit SHA of a fixture repo.
pub fn head_sha(root: &Path) -> String {
    let repo = git2::Repository::open(root).expect("open repo");
    let sha = repo
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("commit")
        .id()
        .to_string();
    sha
}

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let p = root.join(rel);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).expect("mkdir");
        }
        std::fs::write(p, content).expect("write file");
    }
}

/// `remove_dir_all` that first clears read-only attributes — git object files are
/// read-only on Windows and would otherwise fail with EPERM.
fn force_remove_dir_all(p: &Path) {
    fn clear_readonly(p: &Path) {
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            let mut perm = meta.permissions();
            if perm.readonly() {
                // Test-only fixture teardown of a private temp dir — the Unix
                // world-writable concern behind this lint doesn't apply.
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(p, perm);
            }
            if meta.is_dir() {
                if let Ok(rd) = std::fs::read_dir(p) {
                    for e in rd.flatten() {
                        clear_readonly(&e.path());
                    }
                }
            }
        }
    }
    if p.exists() {
        clear_readonly(p);
        let _ = std::fs::remove_dir_all(p);
    }
}
