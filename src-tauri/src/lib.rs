mod diff;
mod git;
mod heuristics;
mod model;
mod parse;
mod rules;
mod session;
mod watcher;

use std::path::Path;

use tauri::{Manager, State};

use model::SessionData;
use watcher::Shared;

// ---------- persistence (last-opened repo) ----------
fn settings_file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("settings.json"))
}

fn save_last_repo(app: &tauri::AppHandle, path: &str) {
    if let Some(file) = settings_file(app) {
        let json = serde_json::json!({ "lastRepoPath": path });
        let _ = std::fs::write(file, serde_json::to_string_pretty(&json).unwrap_or_default());
    }
}

fn load_last_repo(app: &tauri::AppHandle) -> Option<String> {
    let file = settings_file(app)?;
    let text = std::fs::read_to_string(file).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("lastRepoPath")
        .and_then(|v| v.as_str())
        .map(String::from)
}

// ---------- commands ----------
/// Open a repo: validate it's a git repo, persist it, start watching, return the
/// initial analysis. Errors (frontend shows it inline) if the folder isn't a repo.
#[tauri::command]
fn open_repo(app: tauri::AppHandle, shared: State<'_, Shared>, path: String) -> Result<SessionData, String> {
    let root = git::repo_root(Path::new(&path))
        .ok_or_else(|| format!("\"{path}\" isn't a git repository."))?;
    save_last_repo(&app, &root.display().to_string());
    Ok(watcher::start(&app, shared.inner(), root))
}

/// On launch: reopen the last repo if it still exists + is a git repo, else `None`
/// (the frontend shows onboarding).
#[tauri::command]
fn init_session(app: tauri::AppHandle, shared: State<'_, Shared>) -> Result<Option<SessionData>, String> {
    let Some(saved) = load_last_repo(&app) else {
        return Ok(None);
    };
    let Some(root) = git::repo_root(Path::new(&saved)) else {
        return Ok(None);
    };
    Ok(Some(watcher::start(&app, shared.inner(), root)))
}

#[tauri::command]
fn stop_watching(shared: State<'_, Shared>) {
    watcher::stop(shared.inner());
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
        .invoke_handler(tauri::generate_handler![open_repo, init_session, stop_watching])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
