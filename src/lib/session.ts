// Session data layer: drives the real Rust commands in Tauri, and falls back to
// the typed mock in a plain browser so `npm run dev` can exercise the UI/states.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
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
  if (!isTauri) return mockSession;
  return invoke<SessionData>("open_repo", { path });
}

/** On launch: the persisted repo's analysis, or null → show onboarding. */
export async function initSession(): Promise<SessionData | null> {
  if (!isTauri) return null; // browser: start on onboarding so it's testable
  return invoke<SessionData | null>("init_session");
}

export async function stopWatching(): Promise<void> {
  if (isTauri) await invoke("stop_watching");
}

/** Subscribe to live re-analysis pushes from the watcher. */
export function onDrift(cb: (data: SessionData) => void): Promise<UnlistenFn> {
  if (!isTauri) return Promise.resolve(() => {});
  return listen<SessionData>("drift://updated", (e) => cb(e.payload));
}
