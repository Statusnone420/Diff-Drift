// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `diff-drift check …` from a terminal: the release binary has no console
    // (windows subsystem), so attach to the parent's before printing. Output
    // through pipes/redirection (hooks, CI) works regardless.
    #[cfg(all(windows, not(debug_assertions)))]
    if std::env::args().nth(1).is_some() {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
    if let Some(code) = drift_inspector_lib::cli::try_run() {
        std::process::exit(code);
    }
    drift_inspector_lib::run()
}
