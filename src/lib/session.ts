// Session data layer: drives the real Rust commands in Tauri, and falls back to
// the typed mock in a plain browser so `npm run dev` can exercise the UI/states.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { SessionData } from "../types";
import { mockSession } from "../data/mockSession";
import { isTauri } from "./window";

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

/** Approve (or revoke approval of) the current drift; auto-revokes when drift changes. */
export async function setApproved(approved: boolean, approvedAt: string | null): Promise<SessionData> {
  if (!isTauri) return browserSetApproved(approved, approvedAt);
  return invoke<SessionData>("set_approved", { approved, approvedAt });
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
  return structuredClone(d);
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
  return structuredClone(d);
}
