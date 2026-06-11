//! Read-only headless check — the agent-facing half of Diff Drift.
//!
//! `diff-drift check [path] [--json|--md] [--baseline <head|trust-point|merge-base|rev>]`
//!
//! Prints the same SessionData the GUI renders (JSON by default, Markdown with
//! `--md`) and exits with the max ACTIVE severity: 0 none, 1 low, 2 medium,
//! 3 high (64 = usage error). One hook line turns it into a gate: analyze the
//! drift after an agent finishes, block on a non-zero exit. It reads the same
//! per-repo triage state as the GUI (dismissed flags, trust point) and never
//! writes anything.
use std::path::{Path, PathBuf};

use crate::model::Severity;
use crate::{git, report, session, store};

/// Entry point from the GUI exe's `main`: `Some(exit_code)` when the first
/// arg is a CLI subcommand, `None` to launch the GUI.
pub fn try_run() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("check") {
        return None;
    }
    let (code, output) = run_check(&args[1..]);
    write_report(code, &output);
    Some(code)
}

/// Entry point for the `diff-drift-cli` console bin: there is no GUI to fall
/// back to, so anything that isn't `check` (or help) is a usage error.
pub fn run_cli_only() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (code, output) = dispatch_cli(&args);
    write_report(code, &output);
    code
}

/// Top-level arg dispatch for the console bin, pure for tests. No args is a
/// usage error (64), not help: exit 0 means "no active flags", and a gate
/// script that forgot `check` must not read usage output as a clean pass.
fn dispatch_cli(args: &[String]) -> (i32, String) {
    match args.first().map(String::as_str) {
        Some("check") => run_check(&args[1..]),
        Some("--help") | Some("-h") => (0, USAGE.into()),
        _ => (64, USAGE.into()),
    }
}

/// Report to stdout, usage errors to stderr. Failed writes are ignored: the
/// reader side can close mid-print (`check | head`, a dropped pipe), and
/// `println!` would panic on that ("os error 232" on Windows). The exit code
/// already carries the verdict, so a torn report must not turn into a panic.
fn write_report(code: i32, output: &str) {
    if code == 64 {
        write_report_to(&mut std::io::stderr(), output);
    } else {
        write_report_to(&mut std::io::stdout(), output);
    }
}

fn write_report_to(w: &mut impl std::io::Write, output: &str) {
    let _ = writeln!(w, "{output}");
}

/// The check itself, pure enough to test: returns (exit code, stdout text).
pub fn run_check(args: &[String]) -> (i32, String) {
    let mut path = ".".to_string();
    let mut markdown = false;
    let mut baseline_override: Option<String> = None;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--json" => markdown = false,
            "--md" | "--markdown" => markdown = true,
            "--baseline" => match it.next() {
                Some(spec) => baseline_override = Some(spec.clone()),
                None => return (64, USAGE.into()),
            },
            "--help" | "-h" => return (0, USAGE.into()),
            other if !other.starts_with('-') => path = other.to_string(),
            _ => return (64, USAGE.into()),
        }
    }

    let Some(root) = git::repo_root(Path::new(&path)) else {
        return (64, format!("\"{path}\" isn't a git repository.\n\n{USAGE}"));
    };

    // Same triage state the GUI persists; read-only here.
    let mut state = default_state_file()
        .map(|file| store::load(&file, &root.display().to_string()))
        .unwrap_or_default();
    // An explicit `--baseline` is a contract, not a preference: it must resolve
    // or the check fails loudly (64). Without the flag, the persisted GUI choice
    // applies with the GUI's own HEAD-fallback behavior.
    let explicit_baseline = baseline_override.is_some();
    if let Some(spec) = baseline_override {
        state.baseline = match spec.trim() {
            "" | "head" => None,
            other => Some(other.to_string()),
        };
    }

    let baseline = if explicit_baseline {
        match session::resolve_baseline_strict(&root, &state) {
            Ok(b) => b,
            Err(e) => {
                let spec = state.baseline.as_deref().unwrap_or("head");
                return (64, format!("--baseline {spec}: {e}\n\n{USAGE}"));
            }
        }
    } else {
        session::resolve_baseline(&root, &state)
    };
    let results = session::analyze_all(&root, &baseline);
    let data = session::assemble(&results, &session::meta(&root, &baseline), &state);

    let output = if markdown {
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
        report::render_markdown(&data, &format!("headless check (epoch {epoch})"))
    } else {
        report::render_json(&data)
    };

    let code = data
        .flags
        .iter()
        .filter(|f| !f.dismissed)
        .map(|f| match f.severity {
            Severity::High => 3,
            Severity::Medium => 2,
            Severity::Low => 1,
        })
        .max()
        .unwrap_or(0);
    (code, output)
}

const USAGE: &str = "Usage: diff-drift check [path] [--json|--md] [--baseline <head|trust-point|merge-base|rev>]\n\nExit code: 0 no active flags · 1 low · 2 medium · 3 high · 64 usage error.";

/// The GUI's per-repo state file (`app_config_dir/repo-state.json`), resolved
/// without a Tauri runtime. Returns `None` when the platform dir is unknown —
/// the check then runs with no triage state (all flags active).
fn default_state_file() -> Option<PathBuf> {
    const IDENTIFIER: &str = "io.github.statusnone420.diffdrift";
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))?
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
    Some(base.join(IDENTIFIER).join("repo-state.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn check_emits_valid_json_and_severity_exit_code() {
        let fixture = test_fixture::payments_api();
        let (code, out) = run_check(&s(&[fixture.root.to_str().unwrap()]));
        assert_eq!(code, 3, "fixture has a High flag → exit 3");
        let v: serde_json::Value = serde_json::from_str(&out).expect("stdout is valid JSON");
        assert_eq!(v["schemaVersion"], crate::model::SCHEMA_VERSION);
        assert_eq!(v["session"]["riskCount"], 6);
        assert!(v["flags"].as_array().unwrap().len() >= 6);
    }

    #[test]
    fn check_markdown_renders_the_report() {
        let fixture = test_fixture::payments_api();
        let (code, out) = run_check(&s(&[fixture.root.to_str().unwrap(), "--md"]));
        assert_eq!(code, 3);
        assert!(out.contains("# Diff Drift report —"));
        assert!(out.contains("Loose regex pattern"));
    }

    #[test]
    fn check_exit_code_is_zero_on_clean_drift_vs_baseline() {
        let fixture = test_fixture::payments_api();
        let root = fixture.root.to_str().unwrap();
        // Committing everything makes HEAD-baseline drift empty…
        test_fixture::commit_all(&fixture.root, "agent commits");
        let (code, out) = run_check(&s(&[root]));
        assert_eq!(code, 0, "clean tree → exit 0");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["session"]["riskCount"], 0);
        // …but an explicit baseline still sees the committed drift.
        let (code, out) = run_check(&s(&[root, "--baseline", "HEAD~1"]));
        assert_eq!(code, 3, "committed drift visible vs HEAD~1");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["session"]["riskCount"], 6);
        assert_eq!(v["session"]["baselineSpec"], "HEAD~1");
    }

    #[test]
    fn check_rejects_bad_usage() {
        let (code, out) = run_check(&s(&["--nonsense"]));
        assert_eq!(code, 64);
        assert!(out.contains("Usage:"));
        let dir = std::env::temp_dir().join(format!("drift-cli-not-repo-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (code, out) = run_check(&s(&[dir.to_str().unwrap()]));
        assert_eq!(code, 64);
        assert!(out.contains("isn't a git repository"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_help_exits_zero_with_usage() {
        for flag in ["--help", "-h"] {
            let (code, out) = run_check(&s(&[flag]));
            assert_eq!(code, 0, "{flag} is not a usage error");
            assert!(out.contains("Usage:"));
        }
    }

    #[test]
    fn cli_dispatch_maps_top_level_args() {
        // `check` delegates to run_check: the fixture has a High flag → 3.
        let fixture = test_fixture::payments_api();
        let (code, _) = dispatch_cli(&s(&["check", fixture.root.to_str().unwrap()]));
        assert_eq!(code, 3);

        // Help is not an error…
        for flag in ["--help", "-h"] {
            let (code, out) = dispatch_cli(&s(&[flag]));
            assert_eq!(code, 0, "{flag}");
            assert!(out.contains("Usage:"));
        }
        // …but anything else — including no args at all — is usage (64).
        // Exit 0 means "no active flags", so a gate that forgot `check`
        // must not read usage output as a clean pass.
        for args in [&[][..], &["frobnicate"][..], &["--json"][..]] {
            let (code, out) = dispatch_cli(&s(args));
            assert_eq!(code, 64, "{args:?}");
            assert!(out.contains("Usage:"));
        }
    }

    #[test]
    fn report_writes_quietly_into_a_closed_pipe() {
        // The reader side of stdout can vanish mid-print (`check | head`,
        // pwsh closing the pipe). `println!` panics on that ("os error 232"
        // on Windows); the report writer must swallow it — the exit code
        // already carries the verdict.
        struct ClosedPipe;
        impl std::io::Write for ClosedPipe {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        write_report_to(&mut ClosedPipe, "# a report nobody is reading");
    }

    #[test]
    fn check_rejects_unresolvable_explicit_baseline() {
        let fixture = test_fixture::payments_api();
        let root = fixture.root.to_str().unwrap();

        // Unknown custom ref: no silent HEAD fallback for an explicit flag.
        let (code, out) = run_check(&s(&[root, "--baseline", "no-such-ref", "--json"]));
        assert_eq!(code, 64, "unknown ref must fail, not fall back to HEAD");
        assert!(out.contains("doesn't resolve to a commit"), "names the cause: {out}");
        assert!(out.contains("Usage:"), "points at the help text");

        // Trust-point requested but never pinned.
        let (code, out) = run_check(&s(&[root, "--baseline", "trust-point"]));
        assert_eq!(code, 64);
        assert!(out.contains("No trust point yet"), "names the cause: {out}");

        // Merge-base with no default branch to anchor to.
        {
            let repo = git2::Repository::open(&fixture.root).unwrap();
            for name in ["main", "master"] {
                if let Ok(mut b) = repo.find_branch(name, git2::BranchType::Local) {
                    b.delete().unwrap();
                }
            }
        }
        let (code, out) = run_check(&s(&[root, "--baseline", "merge-base"]));
        assert_eq!(code, 64);
        assert!(out.contains("No default branch"), "names the cause: {out}");

        // An explicit baseline that DOES resolve keeps severity-exit semantics.
        let (code, _) = run_check(&s(&[root, "--baseline", "head"]));
        assert_eq!(code, 3, "explicit head still exits by severity");
    }
}
