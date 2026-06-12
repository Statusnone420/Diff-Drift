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

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AstNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub state: NodeState,
    /// Per-node review state (changed nodes only): true while the node's content
    /// matches what the user last marked reviewed. Content drift resets it.
    pub reviewed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<AstNode>>,
}

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Flag {
    pub id: String,
    pub severity: Severity,
    pub r#type: String,
    pub desc: String,
    /// The specific line that triggered the flag, when a rule can point at one.
    /// Surfaced in the report so a truncated node body never hides the evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    pub file_id: String,
    pub file_path: String,
    pub node_path: String,
    pub node_id: String,
    /// Triage state: true when the user dismissed this flag (persisted per repo).
    pub dismissed: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub lang: String,
    pub risks: u32,
    pub summary: String,
    /// True when the file was too large to analyze — listed but not parsed.
    #[serde(skip_serializing_if = "core::ops::Not::not")]
    pub skipped: bool,
    /// Changed (added/modified/removed) nodes in this file, including children.
    pub changed_nodes: u32,
    /// How many of those the user has marked reviewed (content still matching).
    pub reviewed_nodes: u32,
    pub nodes: Vec<AstNode>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub project: String,
    pub branch: String,
    pub repo_path: String,
    /// The user's baseline choice: "head" | "trust-point" | "merge-base" | a rev.
    pub baseline_spec: String,
    /// Short human label for the resolved baseline, e.g. "HEAD", "trust point @ ab12cd3".
    pub baseline_label: String,
    /// Short SHA of the pinned trust point, when one exists (set by "Mark reviewed").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_point: Option<String>,
    pub changed_files: u32,
    pub risk_count: u32,
    pub file_count: u32,
    /// Files listed but not analyzed because they exceed the parse-size cap —
    /// surfaced so "0 active flags" can't read as "fully analyzed clean".
    pub skipped_files: u32,
    /// Review progress across the whole drift: changed nodes vs reviewed nodes.
    pub changed_nodes: u32,
    pub reviewed_nodes: u32,
    /// True while the stored approval matches the current drift fingerprint —
    /// any change to the drift auto-revokes it.
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
}

/// Version of this data contract. Bump when the shape of `SessionData` changes
/// in a way consumers (JSON export, headless check) could misread. v0.1 shipped
/// without the field (implicitly 1); v3 added `session.skippedFiles` and
/// `files[].skipped` for the oversized-file guard; v4 added `otherFiles` (the
/// list of changed paths that are not analyzed as AST or dependency drift).
pub const SCHEMA_VERSION: u32 = 4;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    pub schema_version: u32,
    pub session: Session,
    pub flags: Vec<Flag>,
    pub files: Vec<FileEntry>,
    /// Repo-relative paths of changed files that are not analyzed as AST or
    /// dependency drift (not a supported source language, not package.json),
    /// sorted alphabetically. Empty when every changed file was analyzed.
    pub other_files: Vec<String>,
}
