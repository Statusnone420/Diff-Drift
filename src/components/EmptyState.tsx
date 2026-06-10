import { Ico } from "../lib/icons";

export function EmptyState({ error, onOpen }: { error: string | null; onOpen: () => void }) {
  return (
    <div className="empty-state">
      <div className="es-logo">
        <svg width="34" height="34" viewBox="0 0 16 16" fill="none">
          <path
            d="M2 11.5C4 11.5 4 4.5 8 4.5s4 7 6 7"
            stroke="#e7a83e"
            strokeWidth="1.4"
            strokeLinecap="round"
            fill="none"
          />
          <circle cx="8" cy="4.5" r="1.5" fill="#e7a83e" />
        </svg>
      </div>
      <div className="es-title">Diff Drift</div>
      <div className="es-tagline">
        Open a git repository to inspect the AST-level security drift in its changes — measured
        against the last commit, your last review, or any ref you choose.
      </div>
      <div className="es-steps">
        Pick a baseline → review the changed nodes → dismiss flags or mark the drift reviewed.
      </div>
      <button className="btn primary es-open" onClick={onOpen}>
        {Ico.folder}
        Open a repository…
      </button>
      {error && (
        <div className="es-error">
          {Ico.warn}
          {error}
        </div>
      )}
    </div>
  );
}
