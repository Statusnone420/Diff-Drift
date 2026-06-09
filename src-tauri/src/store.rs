//! Per-repo persisted triage state (`repo-state.json` in the app config dir):
//! which flags the user dismissed, and the drift fingerprint they approved.
//! Keyed by the repo root path so state never leaks between repositories.
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct RepoState {
    pub dismissed: HashSet<String>,
    pub approved_fingerprint: Option<String>,
    pub approved_at: Option<String>,
    /// The user's baseline choice: `"head"` (or absent), `"trust-point"`,
    /// `"merge-base"`, or any git rev string. Resolved per scan in `session.rs`.
    pub baseline: Option<String>,
    /// Last trusted commit SHA — pinned by "Mark reviewed". The drift "since the
    /// agent started" is everything between this commit and the working tree.
    pub trust_point: Option<String>,
    /// Per-node review state: node id → content hash at review time. A node whose
    /// content changes after review automatically reads as unreviewed again —
    /// that IS the "new since last look" signal. Rebuilt (and pruned) whenever
    /// the whole drift is marked reviewed.
    pub reviewed_nodes: BTreeMap<String, String>,
}

impl RepoState {
    pub fn is_empty(&self) -> bool {
        self.dismissed.is_empty()
            && self.approved_fingerprint.is_none()
            && self.baseline.is_none()
            && self.trust_point.is_none()
            && self.reviewed_nodes.is_empty()
    }
}

/// The whole file: repo root → its state. BTreeMap for stable on-disk ordering.
type StateFile = BTreeMap<String, RepoState>;

fn read_all(file: &Path) -> StateFile {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Load the persisted state for one repo (empty default if none).
pub fn load(file: &Path, repo_key: &str) -> RepoState {
    read_all(file).remove(repo_key).unwrap_or_default()
}

/// Persist one repo's state, dropping the entry entirely once it's empty.
/// Write errors are swallowed (triage state is a convenience, not critical data).
pub fn save(file: &Path, repo_key: &str, state: &RepoState) {
    let mut all = read_all(file);
    if state.is_empty() {
        all.remove(repo_key);
    } else {
        all.insert(repo_key.to_string(), state.clone());
    }
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(&all) {
        let _ = std::fs::write(file, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("drift-store-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("repo-state.json")
    }

    #[test]
    fn round_trips_per_repo_state() {
        let file = temp_file("roundtrip");
        let mut state = RepoState::default();
        state.dismissed.insert("rule@node".into());
        state.approved_fingerprint = Some("fp".into());
        state.approved_at = Some("12:30".into());

        save(&file, r"C:\repo-a", &state);
        save(&file, r"C:\repo-b", &RepoState { dismissed: ["x".into()].into(), ..Default::default() });

        assert_eq!(load(&file, r"C:\repo-a"), state);
        assert!(load(&file, r"C:\repo-b").dismissed.contains("x"));
        assert_eq!(load(&file, r"C:\repo-c"), RepoState::default(), "unknown repo → default");
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn empty_state_removes_the_entry() {
        let file = temp_file("prune");
        let mut state = RepoState::default();
        state.dismissed.insert("f".into());
        save(&file, "repo", &state);
        assert!(!load(&file, "repo").dismissed.is_empty());

        save(&file, "repo", &RepoState::default());
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("repo\""), "emptied repo entry should be pruned: {text}");
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn corrupt_file_loads_as_default() {
        let file = temp_file("corrupt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{not json").unwrap();
        assert_eq!(load(&file, "repo"), RepoState::default());
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }
}
