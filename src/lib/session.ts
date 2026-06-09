// Session data layer: drives the real Rust commands in Tauri, and falls back to
// the typed mock in a plain browser so `npm run dev` can exercise the UI/states.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { SessionData } from "../types";
import { mockSession } from "../data/mockSession";
import { isTauri } from "./window";

interface E2eConfig {
  repoPath?: string;
  exportPath?: string;
}

/** Native folder picker → chosen directory path, or null if cancelled. */
export async function pickFolder(): Promise<string | null> {
  if (!isTauri) return "demo"; // browser: pretend a folder was picked → loads mock
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Open a repository",
  });
  return typeof selected === "string" ? selected : null;
}

/** Open + analyze a repo. Rejects (string message) if it isn't a git repo. */
export async function openRepo(path: string): Promise<SessionData> {
  if (!isTauri) return browserMock();
  return invoke<SessionData>("open_repo", { path });
}

/** On launch: the persisted repo's analysis, or null → show onboarding. */
export async function initSession(): Promise<SessionData | null> {
  if (!isTauri) return null; // browser: start on onboarding so it's testable
  const cfg = await e2eConfig();
  if (cfg?.repoPath) return invoke<SessionData>("open_repo", { path: cfg.repoPath });
  return invoke<SessionData | null>("init_session");
}

/** Dismiss (or restore) one flag. Persisted per repo; resolves to the updated session. */
export async function setFlagDismissed(flagId: string, dismissed: boolean): Promise<SessionData> {
  if (!isTauri) return browserSetDismissed(flagId, dismissed);
  return invoke<SessionData>("set_flag_dismissed", { flagId, dismissed });
}

/** Dismiss every currently active flag. */
export async function dismissAll(): Promise<SessionData> {
  if (!isTauri) return browserDismissAll();
  return invoke<SessionData>("dismiss_all");
}

/** Mark one changed node reviewed (or unreviewed). Persisted per repo;
 * auto-resets when the node's content changes afterwards. */
export async function setNodeReviewed(nodeId: string, reviewed: boolean): Promise<SessionData> {
  if (!isTauri) return browserSetNodeReviewed(nodeId, reviewed);
  return invoke<SessionData>("set_node_reviewed", { nodeId, reviewed });
}

/** Approve (or revoke approval of) the current drift; auto-revokes when drift changes.
 * Approving also pins the trust point to the current HEAD commit. */
export async function setApproved(approved: boolean, approvedAt: string | null): Promise<SessionData> {
  if (!isTauri) return browserSetApproved(approved, approvedAt);
  return invoke<SessionData>("set_approved", { approved, approvedAt });
}

/** Switch the baseline the drift is measured against ("head" | "trust-point" |
 * "merge-base" | any git rev). Persisted per repo; rejects with a message when
 * the choice can't resolve (e.g. unknown ref). */
export async function setBaseline(spec: string): Promise<SessionData> {
  if (!isTauri) return browserSetBaseline(spec);
  return invoke<SessionData>("set_baseline", { spec });
}

/**
 * Pick a destination (native save dialog) and write the session report there.
 * Resolves to the written path, or null if the user cancelled.
 */
export async function exportReport(project: string): Promise<string | null> {
  const generatedAt = new Date().toLocaleString();
  if (!isTauri) {
    // Browser dev: download the mock as JSON so the action stays exercisable.
    const blob = new Blob([JSON.stringify(browserData ?? mockSession, null, 2)], {
      type: "application/json",
    });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `diff-drift-${project}.json`;
    a.click();
    URL.revokeObjectURL(a.href);
    return a.download;
  }
  const cfg = await e2eConfig();
  if (cfg?.exportPath) {
    await invoke("export_report", { path: cfg.exportPath, generatedAt });
    return cfg.exportPath;
  }
  const path = await save({
    title: "Export report",
    defaultPath: `diff-drift-${project}.md`,
    filters: [
      { name: "Markdown report", extensions: ["md"] },
      { name: "JSON data", extensions: ["json"] },
    ],
  });
  if (typeof path !== "string") return null;
  await invoke("export_report", { path, generatedAt });
  return path;
}

/** Subscribe to live re-analysis pushes from the watcher. */
export function onDrift(cb: (data: SessionData) => void): Promise<UnlistenFn> {
  if (!isTauri) return Promise.resolve(() => {});
  return listen<SessionData>("drift://updated", (e) => cb(e.payload));
}

let e2eConfigCache: E2eConfig | null | undefined;

async function e2eConfig(): Promise<E2eConfig | null> {
  if (!isTauri) return null;
  if (e2eConfigCache !== undefined) return e2eConfigCache;
  try {
    e2eConfigCache = await invoke<E2eConfig | null>("e2e_config");
  } catch {
    e2eConfigCache = null;
  }
  return e2eConfigCache;
}

// ---------- browser (npm run dev) triage shim ----------
// Keeps the mock honest: dismiss/approve visibly update counts in a plain browser.
// Never bundled into behavior inside Tauri (isTauri guards every entry point).

let browserData: SessionData | null = null;

function browserMock(): SessionData {
  browserData = structuredClone(mockSession);
  return browserData;
}

function browserRecount(d: SessionData): SessionData {
  const active = d.flags.filter((f) => !f.dismissed);
  d.flags = [...active, ...d.flags.filter((f) => f.dismissed)];
  d.files.forEach((file) => {
    file.risks = active.filter((f) => f.fileId === file.id).length;
  });
  d.session.riskCount = active.length;
  d.session.fileCount = d.files.filter((f) => f.risks > 0).length;
  let changed = 0;
  let reviewed = 0;
  const walk = (ns: SessionData["files"][number]["nodes"], file: SessionData["files"][number]) => {
    ns.forEach((n) => {
      if (n.state !== "unchanged") {
        changed++;
        file.changedNodes++;
        if (n.reviewed) {
          reviewed++;
          file.reviewedNodes++;
        }
      }
      if (n.children) walk(n.children, file);
    });
  };
  d.files.forEach((file) => {
    file.changedNodes = 0;
    file.reviewedNodes = 0;
    walk(file.nodes, file);
  });
  d.session.changedNodes = changed;
  d.session.reviewedNodes = reviewed;
  return structuredClone(d);
}

function browserSetNodeReviewed(nodeId: string, reviewed: boolean): SessionData {
  const d = browserData ?? browserMock();
  const walk = (ns: SessionData["files"][number]["nodes"]) => {
    ns.forEach((n) => {
      if (n.id === nodeId) n.reviewed = reviewed;
      if (n.children) walk(n.children);
    });
  };
  d.files.forEach((f) => walk(f.nodes));
  return browserRecount(d);
}

function browserSetDismissed(flagId: string, dismissed: boolean): SessionData {
  const d = browserData ?? browserMock();
  d.flags.forEach((f) => {
    if (f.id === flagId) f.dismissed = dismissed;
  });
  return browserRecount(d);
}

function browserDismissAll(): SessionData {
  const d = browserData ?? browserMock();
  d.flags.forEach((f) => (f.dismissed = true));
  return browserRecount(d);
}

function browserSetApproved(approved: boolean, approvedAt: string | null): SessionData {
  const d = browserData ?? browserMock();
  d.session.approved = approved;
  d.session.approvedAt = approved ? approvedAt ?? undefined : undefined;
  if (!approved) return structuredClone(d);
  // Mirrors the backend: reviewing the drift pins a trust point and reviews every node.
  d.session.trustPoint = "ab12cd3";
  const walk = (ns: SessionData["files"][number]["nodes"]) => {
    ns.forEach((n) => {
      if (n.state !== "unchanged") n.reviewed = true;
      if (n.children) walk(n.children);
    });
  };
  d.files.forEach((f) => walk(f.nodes));
  return browserRecount(d);
}

function browserSetBaseline(spec: string): SessionData {
  const d = browserData ?? browserMock();
  if (spec === "trust-point" && !d.session.trustPoint) {
    // Mirrors the Rust command's string rejection.
    throw "No trust point yet — Mark reviewed pins one.";
  }
  d.session.baselineSpec = spec || "head";
  d.session.baselineLabel =
    spec === "trust-point"
      ? `trust point @ ${d.session.trustPoint}`
      : spec === "merge-base"
        ? "merge-base @ ab12cd3"
        : spec === "head" || spec === ""
          ? "HEAD"
          : `${spec} @ ab12cd3`;
  return structuredClone(d);
}
