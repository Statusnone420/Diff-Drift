// The shape contract for AST data. Phase 1 fills this from mock data; Phase 2's
// Rust parser (tree-sitter) emits exactly this shape. Mirrors the handoff README.

export type NodeState = "added" | "removed" | "modified" | "unchanged";

export interface AstNode {
  id: string;
  kind: string; // "ImportDeclaration", "FunctionDeclaration", ...
  name: string; // display name
  signature?: string; // dim trailing text
  state: NodeState;
  flagId?: string; // ties to a risk flag
  before?: string[]; // removed/old lines (for removed + modified)
  after?: string[]; // added/new lines (for added + modified)
  children?: AstNode[];
}

export type Severity = "high" | "medium" | "low";

export interface Flag {
  id: string;
  severity: Severity;
  type: string;
  desc: string;
  fileId: string;
  filePath: string;
  nodePath: string;
  nodeId: string;
  /** Triage state: dismissed flags are excluded from all counts (persisted per repo). */
  dismissed: boolean;
}

export interface FileEntry {
  id: string;
  name: string;
  dir: string;
  lang: string;
  risks: number;
  summary: string;
  nodes: AstNode[];
}

export interface Session {
  project: string;
  branch: string;
  repoPath: string;
  /** The baseline choice: "head" | "trust-point" | "merge-base" | a git rev. */
  baselineSpec: string;
  /** Short label for the resolved baseline, e.g. "HEAD", "trust point @ ab12cd3". */
  baselineLabel: string;
  /** Short SHA of the pinned trust point (set by "Mark reviewed"), if any. */
  trustPoint?: string;
  changedFiles: number;
  riskCount: number;
  fileCount: number;
  /** True while the stored approval matches the current drift; auto-revokes on change. */
  approved: boolean;
  approvedAt?: string;
}

export interface SessionData {
  /** Data-contract version. v0.1 exports had no field (implicitly 1). */
  schemaVersion: number;
  session: Session;
  flags: Flag[];
  files: FileEntry[];
}
