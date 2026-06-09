// Caption-button + window-state wrapper. Uses the Tauri v2 window API when running
// inside Tauri, and no-ops in a plain browser (so the UI runs under `vite` for fast
// pixel verification against the reference).
import { getCurrentWindow } from "@tauri-apps/api/window";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const appWindow = () => getCurrentWindow();

export async function minimize(): Promise<void> {
  if (isTauri) await appWindow().minimize();
}

export async function toggleMaximize(): Promise<void> {
  if (isTauri) await appWindow().toggleMaximize();
}

export async function closeWindow(): Promise<void> {
  if (isTauri) await appWindow().close();
}

export async function isMaximized(): Promise<boolean> {
  if (!isTauri) return false;
  return appWindow().isMaximized();
}

/** Subscribe to maximize-state changes. Fires once immediately. Returns an unsubscribe. */
export function onMaximizeChange(cb: (maximized: boolean) => void): () => void {
  if (!isTauri) return () => {};
  const w = appWindow();
  let unlisten: (() => void) | undefined;
  void w.isMaximized().then(cb);
  void w
    .onResized(() => {
      void w.isMaximized().then(cb);
    })
    .then((u) => {
      unlisten = u;
    });
  return () => unlisten?.();
}
