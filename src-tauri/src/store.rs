//! Per-repo persisted triage state (`repo-state.json` in the app config dir):
//! which flags the user dismissed, and the drift fingerprint they approved.
//! Keyed by the repo root path so state never leaks between repositories.
use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct RepoState {
    /// Dismissed flag id → content hash of the flagged node at dismissal time.
    /// A dismissal only applies while the node's content still matches — a
    /// meaningfully changed node resurfaces its flag. The empty hash marks
    /// legacy v0.2.0 id-only entries; the GUI pins those to current content on
    /// next open, while read-only callers treat them conservatively.
    #[serde(deserialize_with = "de_dismissed")]
    pub dismissed: BTreeMap<String, String>,
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

/// Accepts both the current map shape and the legacy v0.2.0 array of flag ids.
/// Unambiguous: the old format is a JSON array, the new one a JSON object.
fn de_dismissed<'de, D>(d: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Hashed(BTreeMap<String, String>),
        Legacy(Vec<String>),
    }
    Ok(match Repr::deserialize(d)? {
        Repr::Hashed(map) => map,
        Repr::Legacy(ids) => ids.into_iter().map(|id| (id, String::new())).collect(),
    })
}

const STORE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct StateFile {
    version: u32,
    repos: BTreeMap<String, RepoState>,
}

impl Default for StateFile {
    fn default() -> Self {
        StateFile {
            version: STORE_VERSION,
            repos: BTreeMap::new(),
        }
    }
}

fn read_all(file: &Path) -> StateFile {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Versioned(StateFile),
        Legacy(BTreeMap<String, RepoState>),
    }

    match std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Repr>(&text).ok())
    {
        Some(Repr::Versioned(mut state)) => {
            state.version = STORE_VERSION;
            state
        }
        Some(Repr::Legacy(repos)) => StateFile {
            version: STORE_VERSION,
            repos,
        },
        None => StateFile::default(),
    }
}

/// Load the persisted state for one repo (empty default if none).
pub fn load(file: &Path, repo_key: &str) -> RepoState {
    read_all(file).repos.remove(repo_key).unwrap_or_default()
}

/// Persist one repo's state, dropping the entry entirely once it's empty.
/// Write errors are swallowed (triage state is a convenience, not critical data).
pub fn save(file: &Path, repo_key: &str, state: &RepoState) {
    let mut all = read_all(file);
    if state.is_empty() {
        all.repos.remove(repo_key);
    } else {
        all.repos.insert(repo_key.to_string(), state.clone());
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
        let dir =
            std::env::temp_dir().join(format!("drift-store-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("repo-state.json")
    }

    #[test]
    fn round_trips_per_repo_state() {
        let file = temp_file("roundtrip");
        let mut state = RepoState::default();
        state.dismissed.insert("rule@node".into(), "abc123".into());
        state.approved_fingerprint = Some("fp".into());
        state.approved_at = Some("12:30".into());

        save(&file, r"C:\repo-a", &state);
        save(
            &file,
            r"C:\repo-b",
            &RepoState {
                dismissed: [("x".into(), String::new())].into(),
                ..Default::default()
            },
        );

        assert_eq!(load(&file, r"C:\repo-a"), state);
        assert!(load(&file, r"C:\repo-b").dismissed.contains_key("x"));
        assert_eq!(
            load(&file, r"C:\repo-c"),
            RepoState::default(),
            "unknown repo → default"
        );
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn legacy_dismissed_array_still_loads() {
        // A literal v0.2.0 state file: `dismissed` was a plain array of flag ids.
        let file = temp_file("legacy");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"{ "C:\\repo": { "dismissed": ["loose-regex@auth-ts:0", "eval@shim-js:0"], "trustPoint": "abc" } }"#,
        )
        .unwrap();

        let state = load(&file, r"C:\repo");
        assert_eq!(state.dismissed.len(), 2);
        // Legacy ids carry an empty hash until the GUI pins them.
        assert_eq!(
            state
                .dismissed
                .get("loose-regex@auth-ts:0")
                .map(String::as_str),
            Some("")
        );
        assert_eq!(state.trust_point.as_deref(), Some("abc"));

        // Saving rewrites in the new map shape and still loads.
        save(&file, r"C:\repo", &state);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains(r#""version": 1"#),
            "versioned wrapper: {text}"
        );
        assert!(text.contains(r#""repos": {"#), "repo map is nested: {text}");
        assert!(
            text.contains(r#""loose-regex@auth-ts:0": """#),
            "map shape on disk: {text}"
        );
        assert_eq!(load(&file, r"C:\repo"), state);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn unversioned_legacy_file_migrates_to_versioned_on_save() {
        let file = temp_file("legacy-wrapper");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"{ "C:\\repo": { "dismissed": ["legacy-flag"], "baseline": "trust-point", "trustPoint": "abc123" } }"#,
        )
        .unwrap();

        let mut state = load(&file, r"C:\repo");
        assert_eq!(
            state.dismissed.get("legacy-flag").map(String::as_str),
            Some("")
        );
        state.approved_at = Some("12:30".into());
        save(&file, r"C:\repo", &state);

        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains(r#""version": 1"#),
            "version field written: {text}"
        );
        assert!(
            text.contains(r#""repos": {"#),
            "legacy map nested under repos: {text}"
        );
        assert!(
            text.contains(r#""legacy-flag": """#),
            "dismissal array migrated: {text}"
        );
        assert_eq!(load(&file, r"C:\repo"), state);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn versioned_file_round_trips() {
        let file = temp_file("versioned");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"{ "version": 1, "repos": { "repo": { "dismissed": { "flag": "hash" }, "reviewedNodes": { "node": "hash" } } } }"#,
        )
        .unwrap();

        let state = load(&file, "repo");
        assert_eq!(
            state.dismissed.get("flag").map(String::as_str),
            Some("hash")
        );
        assert_eq!(
            state.reviewed_nodes.get("node").map(String::as_str),
            Some("hash")
        );
        save(&file, "repo", &state);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(r#""version": 1"#));
        assert_eq!(load(&file, "repo"), state);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn empty_state_removes_the_entry() {
        let file = temp_file("prune");
        let mut state = RepoState::default();
        state.dismissed.insert("f".into(), String::new());
        save(&file, "repo", &state);
        assert!(!load(&file, "repo").dismissed.is_empty());

        save(&file, "repo", &RepoState::default());
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("repo\""),
            "emptied repo entry should be pruned: {text}"
        );
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
