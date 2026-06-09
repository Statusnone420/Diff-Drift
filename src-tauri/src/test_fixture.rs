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
