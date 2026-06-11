//! Integration tests for the `diff-drift-cli` console binary.
//!
//! These spawn the real compiled exe (cargo provides its path via
//! `CARGO_BIN_EXE_diff-drift-cli`) and assert on actual process exit codes —
//! the same contract PowerShell's `$LASTEXITCODE`, cmd's `%ERRORLEVEL%`, and
//! `sh` see. The GUI exe is windows-subsystem in release, so shells don't wait
//! for it; this dedicated console bin is what scripts and CI are pointed at.
use std::path::Path;
use std::process::{Command, Output};

fn cli(args: &[&str], cwd: &Path) -> Output {
    // Redirect the config home (APPDATA on Windows, HOME/XDG elsewhere) into
    // a throwaway dir so the developer's real repo-state.json is never read.
    let state_home = std::env::temp_dir().join(format!("drift-cli-bin-state-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&state_home);
    Command::new(env!("CARGO_BIN_EXE_diff-drift-cli"))
        .args(args)
        .current_dir(cwd)
        .env("APPDATA", &state_home)
        .env("HOME", &state_home)
        .env("XDG_CONFIG_HOME", &state_home)
        .output()
        .expect("spawn diff-drift-cli")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A unique temp dir with one committed TS file, cleaned up on drop.
struct TempRepo {
    root: std::path::PathBuf,
}

impl TempRepo {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("drift-cli-bin-{tag}-{}", std::process::id()));
        force_remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create repo dir");
        std::fs::write(
            root.join("handler.ts"),
            "function handler(input: string): string {\n  validateInput(input);\n  return input.trim();\n}\n",
        )
        .expect("write file");

        let repo = git2::Repository::init(&root).expect("git init");
        let mut index = repo.index().expect("index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("git add -A");
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("Drift Demo", "demo@drift.local").expect("signature");
        repo.commit(Some("HEAD"), &sig, &sig, "baseline", &tree, &[])
            .expect("commit");
        TempRepo { root }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        force_remove_dir_all(&self.root);
    }
}

/// git object files are read-only on Windows; clear that before removal.
fn force_remove_dir_all(p: &Path) {
    fn clear_readonly(p: &Path) {
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            let mut perm = meta.permissions();
            if perm.readonly() {
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

#[test]
fn help_exits_zero_with_usage_on_stdout() {
    let cwd = std::env::temp_dir();
    for flag in ["--help", "-h"] {
        let out = cli(&[flag], &cwd);
        assert_eq!(out.status.code(), Some(0), "{flag}: {}", stderr(&out));
        assert!(stdout(&out).contains("Usage:"), "{flag} prints usage");
    }
}

#[test]
fn no_subcommand_is_a_usage_error_not_a_gui_launch() {
    let cwd = std::env::temp_dir();
    // Must terminate with 64 (a hung GUI here would time the suite out) and
    // must not exit 0: scripts that forgot `check` can't get a false pass.
    for args in [&[][..], &["frobnicate"][..]] {
        let out = cli(args, &cwd);
        assert_eq!(out.status.code(), Some(64), "{args:?}");
        assert!(stderr(&out).contains("Usage:"), "{args:?} explains usage");
    }
}

#[test]
fn clean_repo_exits_zero_and_prints_json() {
    let repo = TempRepo::new("clean");
    let out = cli(&["check", "."], &repo.root);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("stdout is JSON");
    assert_eq!(v["session"]["riskCount"], 0);
}

#[test]
fn high_severity_drift_exits_three() {
    let repo = TempRepo::new("drift");
    // Uncommitted new file with a loose regex — a known High flag.
    std::fs::write(repo.root.join("parser.ts"), "const parser = /.*/;\n").expect("write drift");
    let out = cli(&["check", ".", "--md"], &repo.root);
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("# Diff Drift report —"));
}
