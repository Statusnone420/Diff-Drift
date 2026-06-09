import { minimize, toggleMaximize, closeWindow } from "../lib/window";

// Mica title bar with a custom drag region + caption buttons wired to the Tauri
// window API. `data-tauri-drag-region` is applied to the bar and the left cluster
// (it does not cascade to children in v2); caption buttons stay interactive.
export function TitleBar({ maximized }: { maximized: boolean }) {
  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="tb-left" data-tauri-drag-region>
        <span className="tb-logo" data-tauri-drag-region>
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
            <path
              d="M2 11.5C4 11.5 4 4.5 8 4.5s4 7 6 7"
              stroke="#e7a83e"
              strokeWidth="1.5"
              strokeLinecap="round"
              fill="none"
            />
            <circle cx="8" cy="4.5" r="1.5" fill="#e7a83e" />
          </svg>
        </span>
        <span className="tb-title" data-tauri-drag-region>
          Diff Drift
        </span>
      </div>
      <div className="tb-caption">
        <div className="cap-btn" title="Minimize" onClick={() => void minimize()}>
          <svg width="10" height="10" viewBox="0 0 10 10">
            <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
          </svg>
        </div>
        <div
          className="cap-btn"
          title={maximized ? "Restore" : "Maximize"}
          onClick={() => void toggleMaximize()}
        >
          {maximized ? (
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <rect x="0.5" y="3" width="6.5" height="6.5" rx="0.5" stroke="currentColor" strokeWidth="1" />
              <path
                d="M3 3V1.5a1 1 0 0 1 1-1h4.5a1 1 0 0 1 1 1V6a1 1 0 0 1-1 1H7"
                stroke="currentColor"
                strokeWidth="1"
              />
            </svg>
          ) : (
            <svg width="10" height="10" viewBox="0 0 10 10">
              <rect x="0.5" y="0.5" width="9" height="9" rx="0.5" stroke="currentColor" strokeWidth="1" fill="none" />
            </svg>
          )}
        </div>
        <div className="cap-btn close" title="Close" onClick={() => void closeWindow()}>
          <svg width="10" height="10" viewBox="0 0 10 10">
            <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" strokeWidth="1" />
          </svg>
        </div>
      </div>
    </div>
  );
}
