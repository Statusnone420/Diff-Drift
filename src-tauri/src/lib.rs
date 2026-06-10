pub mod cli;
mod deps_diff;
pub mod diff;
mod git;
mod heuristics;
pub mod model;
pub mod parse;
mod report;
mod rules;
pub mod session;
pub mod store;
mod structural;
#[cfg(test)]
mod test_fixture;
mod watcher;

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{Manager, State};

use model::SessionData;
use watcher::Shared;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct E2eConfig {
    repo_path: Option<String>,
    export_path: Option<String>,
}

// ---------- persistence (last-opened repo) ----------
fn e2e_env(name: &str) -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn e2e_enabled() -> bool {
    cfg!(debug_assertions)
        && [
            "DIFF_DRIFT_E2E_REPO",
            "DIFF_DRIFT_E2E_EXPORT_PATH",
            "DIFF_DRIFT_E2E_STATE_FILE",
        ]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
}

fn e2e_repo_path() -> Option<String> {
    e2e_env("DIFF_DRIFT_E2E_REPO")
}

fn e2e_export_path() -> Option<String> {
    e2e_env("DIFF_DRIFT_E2E_EXPORT_PATH")
}

fn e2e_state_file() -> Option<PathBuf> {
    if !e2e_enabled() {
        return None;
    }
    if let Some(path) = e2e_env("DIFF_DRIFT_E2E_STATE_FILE") {
        return Some(PathBuf::from(path));
    }
    Some(std::env::temp_dir().join(format!("diff-drift-e2e-state-{}.json", std::process::id())))
}

fn settings_file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("settings.json"))
}

fn save_last_repo(app: &tauri::AppHandle, path: &str) {
    if e2e_enabled() {
        return;
    }
    if let Some(file) = settings_file(app) {
        let json = serde_json::json!({ "lastRepoPath": path });
        let _ = std::fs::write(
            file,
            serde_json::to_string_pretty(&json).unwrap_or_default(),
        );
    }
}

fn load_last_repo(app: &tauri::AppHandle) -> Option<String> {
    if e2e_enabled() {
        return None;
    }
    let file = settings_file(app)?;
    let text = std::fs::read_to_string(file).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("lastRepoPath")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Where per-repo triage state (dismissed flags, approvals) persists.
fn state_file(app: &tauri::AppHandle) -> std::path::PathBuf {
    if let Some(file) = e2e_state_file() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        return file;
    }
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("repo-state.json"))
        .unwrap_or_else(|_| std::path::PathBuf::from("repo-state.json"))
}

// ---------- commands ----------
/// Open a repo: validate it's a git repo, persist it, start watching, return the
/// initial analysis. Errors (frontend shows it inline) if the folder isn't a repo.
#[tauri::command]
fn open_repo(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    path: String,
) -> Result<SessionData, String> {
    let root = git::repo_root(Path::new(&path))
        .ok_or_else(|| format!("\"{path}\" isn't a git repository."))?;
    save_last_repo(&app, &root.display().to_string());
    let state = state_file(&app);
    Ok(watcher::start(&app, shared.inner(), root, state))
}

/// On launch: reopen the last repo if it still exists + is a git repo, else `None`
/// (the frontend shows onboarding).
#[tauri::command]
fn init_session(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
) -> Result<Option<SessionData>, String> {
    if let Some(path) = e2e_repo_path() {
        let root = git::repo_root(Path::new(&path))
            .ok_or_else(|| format!("\"{path}\" isn't a git repository."))?;
        let state = state_file(&app);
        return Ok(Some(watcher::start(&app, shared.inner(), root, state)));
    }
    let Some(saved) = load_last_repo(&app) else {
        return Ok(None);
    };
    let Some(root) = git::repo_root(Path::new(&saved)) else {
        return Ok(None);
    };
    let state = state_file(&app);
    Ok(Some(watcher::start(&app, shared.inner(), root, state)))
}

/// Dismiss (or restore) a single flag. Persisted per repo; returns the updated session.
#[tauri::command]
fn set_flag_dismissed(
    shared: State<'_, Shared>,
    flag_id: String,
    dismissed: bool,
) -> Result<SessionData, String> {
    watcher::set_dismissed(shared.inner(), flag_id, dismissed)
}

/// Dismiss every currently active flag.
#[tauri::command]
fn dismiss_all(shared: State<'_, Shared>) -> Result<SessionData, String> {
    watcher::dismiss_all(shared.inner())
}

/// Mark a single changed node reviewed (or unreviewed). Persisted per repo;
/// auto-resets if the node's content changes afterwards.
#[tauri::command]
fn set_node_reviewed(
    shared: State<'_, Shared>,
    node_id: String,
    reviewed: bool,
) -> Result<SessionData, String> {
    watcher::set_node_reviewed(shared.inner(), node_id, reviewed)
}

/// Switch the drift baseline: "head", "trust-point", "merge-base", or any git
/// rev. Persisted per repo; re-analyzes everything and returns the new session.
#[tauri::command]
fn set_baseline(shared: State<'_, Shared>, spec: String) -> Result<SessionData, String> {
    watcher::set_baseline(shared.inner(), spec)
}

/// Approve (or revoke approval of) the current drift. Approval stores the drift
/// fingerprint, pins the trust point to the current HEAD, and auto-revokes when
/// the drift changes.
#[tauri::command]
fn set_approved(
    shared: State<'_, Shared>,
    approved: bool,
    approved_at: Option<String>,
) -> Result<SessionData, String> {
    watcher::set_approved(shared.inner(), approved, approved_at)
}

/// Write a report of the current session to `path` — JSON when the extension is
/// `.json`, Markdown otherwise.
#[tauri::command]
fn export_report(
    shared: State<'_, Shared>,
    path: String,
    generated_at: String,
) -> Result<(), String> {
    let data = watcher::current_data(shared.inner())?;
    let content = if path.to_lowercase().ends_with(".json") {
        report::render_json(&data)
    } else {
        report::render_markdown(&data, &generated_at)
    };
    std::fs::write(&path, content).map_err(|e| format!("Couldn't write \"{path}\": {e}"))
}

/// Debug-build-only E2E config. Production releases return `None` even if these
/// environment variables exist.
#[tauri::command]
fn e2e_config() -> Option<E2eConfig> {
    if !e2e_enabled() {
        return None;
    }
    Some(E2eConfig {
        repo_path: e2e_repo_path(),
        export_path: e2e_export_path(),
    })
}

// ---------- Win11 chrome ----------
// Round the undecorated window's corners + restore the native drop shadow via DWM.
#[cfg(target_os = "windows")]
fn apply_win11_chrome(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DWM_WINDOW_CORNER_PREFERENCE,
    };
    if let Ok(hwnd) = window.hwnd() {
        let hwnd = HWND(hwnd.0);
        let pref: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &pref as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            );
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(watcher::new_shared())
        .setup(|app| {
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                apply_win11_chrome(&window);
            }
            let _ = app;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_repo,
            init_session,
            set_flag_dismissed,
            dismiss_all,
            set_node_reviewed,
            set_baseline,
            set_approved,
            export_report,
            e2e_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
