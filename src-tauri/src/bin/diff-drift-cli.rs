//! `diff-drift-cli` — console-subsystem build of the read-only `check` CLI.
//!
//! The main `diff-drift` exe is windows-subsystem in release (no console
//! window for the app), which means PowerShell and cmd start it WITHOUT
//! waiting: `$LASTEXITCODE`/`%ERRORLEVEL%` are never set and output capture
//! races. This bin is the same `check` (same lib, same exit codes 0-3/64),
//! compiled as a console app so every shell waits and sees the real exit
//! code. It is what the installer ships next to the app, what releases
//! publish as the bare CLI asset, and what the GitHub Action runs.
fn main() {
    std::process::exit(diff_drift_lib::cli::run_cli_only());
}
