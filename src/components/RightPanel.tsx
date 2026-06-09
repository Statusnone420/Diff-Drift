import { useEffect, useRef, useState } from "react";
import type { Flag } from "../types";
import { Ico, SEV_LABEL } from "../lib/icons";

interface RightPanelProps {
  flags: Flag[];
  activeFlagId: string | null;
  onSelectFlag: (flagId: string) => void;
  onDismissFlag: (flagId: string, dismissed: boolean) => void;
  /** Export the session report. Resolves true when a file was written. */
  onExport: () => Promise<boolean>;
}

function FlagCard({
  flag,
  active,
  onSelect,
  onDismiss,
}: {
  flag: Flag;
  active: boolean;
  onSelect: () => void;
  onDismiss: () => void;
}) {
  return (
    <div
      className={
        "flag " + flag.severity + (active ? " active" : "") + (flag.dismissed ? " dismissed" : "")
      }
    >
      <div className="flag-bar" />
      <button
        type="button"
        className="flag-in flag-select"
        aria-label={`${SEV_LABEL[flag.severity]} severity: ${flag.type} in ${flag.filePath} — ${flag.nodePath}`}
        onClick={onSelect}
      >
        <div className="flag-top">
          <span className="sev-badge">
            <span className="ic">{Ico.warn}</span>
            {SEV_LABEL[flag.severity]}
          </span>
          <span className="flag-type">{flag.type}</span>
        </div>
        <div className="flag-desc">{flag.desc}</div>
        <div className="flag-map">
          <span className="flag-loc">
            <span className="fp">{flag.filePath}</span>
            <span className="np">{flag.nodePath}</span>
          </span>
          <span className="flag-jump">{Ico.jump} view node</span>
        </div>
      </button>
      <button
        className="flag-dismiss"
        aria-label={flag.dismissed ? `Restore flag: ${flag.type}` : `Dismiss flag: ${flag.type}`}
        title={flag.dismissed ? "Restore this flag" : "Dismiss this flag (persisted for this repo)"}
        onClick={onDismiss}
      >
        {flag.dismissed ? Ico.undo : Ico.close}
      </button>
    </div>
  );
}

export function RightPanel({ flags, activeFlagId, onSelectFlag, onDismissFlag, onExport }: RightPanelProps) {
  const active = flags.filter((f) => !f.dismissed);
  const dismissed = flags.filter((f) => f.dismissed);
  const [showDismissed, setShowDismissed] = useState(false);
  const [exportState, setExportState] = useState<"idle" | "busy" | "done">("idle");
  const exportTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {
    if (exportTimer.current) clearTimeout(exportTimer.current);
  }, []);

  const exportNow = async () => {
    if (exportState === "busy") return;
    setExportState("busy");
    const written = await onExport();
    setExportState(written ? "done" : "idle");
    if (written) {
      if (exportTimer.current) clearTimeout(exportTimer.current);
      exportTimer.current = setTimeout(() => setExportState("idle"), 2500);
    }
  };

  return (
    <div className="col right">
      <div className="rp-head">
        <span className="rp-title">
          {Ico.flag}Risk Flags <span className="tcount">{active.length}</span>
        </span>
      </div>
      <div className="col-scroll">
        {active.length === 0 ? (
          <div className="rp-empty">
            <span className="rp-empty-ic">{Ico.check}</span>
            <div className="rp-empty-title">No active risk flags</div>
            <div className="rp-empty-sub">
              {dismissed.length > 0
                ? "Every flag in this drift has been dismissed."
                : "Nothing suspicious in this drift."}
            </div>
          </div>
        ) : (
          <div className="flag-list">
            {active.map((fl) => (
              <FlagCard
                key={fl.id}
                flag={fl}
                active={activeFlagId === fl.id}
                onSelect={() => onSelectFlag(fl.id)}
                onDismiss={() => onDismissFlag(fl.id, true)}
              />
            ))}
          </div>
        )}
        {dismissed.length > 0 && (
          <div className="dismissed-section">
            <button
              className="dismissed-toggle"
              aria-expanded={showDismissed}
              onClick={() => setShowDismissed((s) => !s)}
            >
              <span className={"chev" + (showDismissed ? " open" : "")}>{Ico.chevron}</span>
              Dismissed <span className="tcount">{dismissed.length}</span>
            </button>
            {showDismissed && (
              <div className="flag-list">
                {dismissed.map((fl) => (
                  <FlagCard
                    key={fl.id}
                    flag={fl}
                    active={activeFlagId === fl.id}
                    onSelect={() => onSelectFlag(fl.id)}
                    onDismiss={() => onDismissFlag(fl.id, false)}
                  />
                ))}
              </div>
            )}
          </div>
        )}
      </div>
      <div className="rp-foot">
        <div className="rp-note">Heuristic checks on TypeScript/TSX drift — review, don't trust blindly.</div>
        <button
          className="btn primary rp-export"
          onClick={() => void exportNow()}
          disabled={exportState === "busy"}
          title="Save this session as a Markdown or JSON report"
        >
          {exportState === "done" ? <>{Ico.check} Exported</> : "Export report"}
        </button>
      </div>
    </div>
  );
}
