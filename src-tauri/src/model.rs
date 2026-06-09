//! Serde data model — mirrors the frontend `src/types.ts` contract exactly.
//! `analyze_session` returns `SessionData`; the React app renders it unchanged.
use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
    Added,
    Removed,
    Modified,
    Unchanged,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AstNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub state: NodeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<AstNode>>,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Flag {
    pub id: String,
    pub severity: Severity,
    pub r#type: String,
    pub desc: String,
    pub file_id: String,
    pub file_path: String,
    pub node_path: String,
    pub node_id: String,
    /// Triage state: true when the user dismissed this flag (persisted per repo).
    pub dismissed: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub lang: String,
    pub risks: u32,
    pub summary: String,
    pub nodes: Vec<AstNode>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub project: String,
    pub branch: String,
    pub repo_path: String,
    pub changed_files: u32,
    pub risk_count: u32,
    pub file_count: u32,
    /// True while the stored approval matches the current drift fingerprint —
    /// any change to the drift auto-revokes it.
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct SessionData {
    pub session: Session,
    pub flags: Vec<Flag>,
    pub files: Vec<FileEntry>,
}
