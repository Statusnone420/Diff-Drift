//! Subprocess tests of the packaged `diff-drift check` binary: spawn the real
//! exe against a temp git repo and assert the exit-code contract that scripts
//! and agent hooks rely on (0/1/2/3 by severity, 64 usage error, read-only).
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

const BIN: &str = env!("CARGO_BIN_EXE_diff-drift");

/// A committed one-file repo plus an isolated fake config home, both in a
/// unique temp dir so tests run in parallel and never read the developer's
/// real repo-state.json.
struct CliFixture {
    root: PathBuf,
    repo: PathBuf,
    state_home: PathBuf,
}

impl CliFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "diff-drift-cli-e2e-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        force_remove_dir_all(&root);
        let repo = root.join("repo");
        let state_home = root.join("config-home");
        std::fs::create_dir_all(&repo).expect("create repo dir");
        std::fs::create_dir_all(&state_home).expect("create state home");

        std::fs::write(
            repo.join("auth.ts"),
            "function validate(token: string): boolean {\n  sanitizeInput(token);\n  return verify(token, PUBLIC_KEY);\n}\n",
        )
        .expect("write baseline file");
        let git = git2::Repository::init(&repo).expect("git init");
        let mut index = git.index().expect("index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("git add -A");
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("write tree");
        let tree = git.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("CLI E2E", "e2e@drift.local").expect("signature");
        git.commit(Some("HEAD"), &sig, &sig, "baseline", &tree, &[])
            .expect("commit");

        CliFixture {
            root,
            repo,
            state_home,
        }
    }

    /// Uncommitted High-severity drift (the loose-regex rule).
    fn add_high_drift(&self) {
        std::fs::write(self.repo.join("parser.ts"), "const parser = /.*/;\n").expect("write drift");
    }

    fn state_file(&self) -> PathBuf {
        self.state_home
            .join("io.github.statusnone420.diffdrift")
            .join("repo-state.json")
    }

    /// Run `diff-drift check <repo> <args…>` with the config home redirected
    /// into the fixture (APPDATA on Windows, HOME/XDG elsewhere).
    fn check(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .arg("check")
            .arg(&self.repo)
            .args(args)
            .env("APPDATA", &self.state_home)
            .env("HOME", &self.state_home)
            .env("XDG_CONFIG_HOME", &self.state_home)
            .output()
            .expect("spawn diff-drift")
    }
}

impl Drop for CliFixture {
    fn drop(&mut self) {
        force_remove_dir_all(&self.root);
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn bin_check_emits_valid_json_with_severity_exit() {
    let fx = CliFixture::new();
    fx.add_high_drift();
    let out = fx.check(&["--json"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "High flag → exit 3\n{}",
        stderr(&out)
    );
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("stdout is valid JSON");
    assert!(v["schemaVersion"].is_number());
    assert_eq!(v["session"]["riskCount"], 1);
    assert_eq!(v["flags"][0]["type"], "Loose regex pattern");
    assert!(stderr(&out).is_empty(), "clean stderr on success");
}

#[test]
fn bin_check_clean_repo_exits_zero() {
    let fx = CliFixture::new();
    let out = fx.check(&["--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "no drift → exit 0\n{}",
        stderr(&out)
    );
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["session"]["riskCount"], 0);
}

#[test]
fn bin_check_rejects_unresolvable_explicit_baseline() {
    let fx = CliFixture::new();
    fx.add_high_drift();
    let out = fx.check(&["--baseline", "no-such-ref", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "explicit bad baseline must fail, not fall back"
    );
    assert!(
        stderr(&out).contains("doesn't resolve to a commit"),
        "stderr names the cause: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).is_empty(),
        "no report on stdout when the baseline is invalid"
    );

    let out = fx.check(&["--baseline", "trust-point"]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "trust-point with none pinned must fail"
    );
    assert!(
        stderr(&out).contains("No trust point yet"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn bin_check_help_exits_zero() {
    let fx = CliFixture::new();
    // `check --help` (path-free) goes through the same arg parser.
    let out = Command::new(BIN)
        .args(["check", "--help"])
        .env("APPDATA", &fx.state_home)
        .env("HOME", &fx.state_home)
        .env("XDG_CONFIG_HOME", &fx.state_home)
        .output()
        .expect("spawn diff-drift");
    assert_eq!(out.status.code(), Some(0), "--help is not a usage error");
    assert!(stdout(&out).contains("Usage: diff-drift check"));
}

#[test]
fn bin_check_writes_nothing() {
    let fx = CliFixture::new();
    fx.add_high_drift();
    let out = fx.check(&["--json"]);
    assert_eq!(out.status.code(), Some(3));
    let leftovers: Vec<_> = std::fs::read_dir(&fx.state_home)
        .expect("state home still exists")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(
        leftovers.is_empty(),
        "the CLI is read-only and must not create state: {leftovers:?}"
    );
}

#[test]
fn bin_check_does_not_honor_legacy_wildcard_dismissals() {
    let fx = CliFixture::new();
    fx.add_high_drift();

    let out = fx.check(&["--json"]);
    assert_eq!(out.status.code(), Some(3));
    let first: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let repo_key = first["session"]["repoPath"]
        .as_str()
        .expect("repo path")
        .to_string();
    let flag_id = first["flags"][0]["id"]
        .as_str()
        .expect("flag id")
        .to_string();

    let state_file = fx.state_file();
    std::fs::create_dir_all(state_file.parent().unwrap()).expect("state dir");
    let mut repos = serde_json::Map::new();
    repos.insert(
        repo_key,
        serde_json::json!({
            "dismissed": [flag_id]
        }),
    );
    std::fs::write(
        &state_file,
        serde_json::to_string_pretty(&serde_json::Value::Object(repos)).unwrap(),
    )
    .expect("write legacy state");

    std::fs::write(fx.repo.join("parser.ts"), "const parser = /.+/;\n").expect("rewrite drift");
    let out = fx.check(&["--json"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "legacy id-only dismissal must not hide current CLI flags"
    );
    let second: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(second["session"]["riskCount"], 1);
    assert_eq!(second["flags"][0]["dismissed"], false);
}

/// `remove_dir_all` that clears read-only attributes first — git object files
/// are read-only on Windows and would otherwise fail with EPERM.
fn force_remove_dir_all(p: &Path) {
    fn clear_readonly(p: &Path) {
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            let mut perm = meta.permissions();
            if perm.readonly() {
                // Test-only teardown of a private temp dir.
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
