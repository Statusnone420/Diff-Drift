import { useState } from "react";
import type { Session } from "../types";
import { Ico } from "../lib/icons";

interface ToolbarProps {
  session: Session;
  onSwitchRepo: () => void;
  onDismissAll: () => void;
  onToggleApprove: () => void;
  onSetBaseline: (spec: string) => void;
}

const KNOWN_BASELINES = ["head", "trust-point", "merge-base"];

export function Toolbar({ session, onSwitchRepo, onDismissAll, onToggleApprove, onSetBaseline }: ToolbarProps) {
  const zero = session.riskCount === 0;
  const noDrift = session.changedFiles === 0;
  const isCustom = !KNOWN_BASELINES.includes(session.baselineSpec);
  const [refOpen, setRefOpen] = useState(false);
  const [refValue, setRefValue] = useState("");

  const selectValue = refOpen || isCustom ? "custom" : session.baselineSpec;
  return (
    <div className="toolbar">
      <div className="crumb">
        <button className="proj proj-btn" onClick={onSwitchRepo} title="Open a different repository">
          {session.project}
          <span className="proj-folder">{Ico.folder}</span>
        </button>
        <span className="sep">·</span>
        <span className="branch">
          {Ico.branch}
          {session.branch}
        </span>
        <span className="sep">·</span>
        <span className="baseline" title={`Drift is measured against ${session.baselineLabel}`}>
          <span className="baseline-vs">vs</span>
          <select
            className="baseline-select"
            aria-label="Baseline to diff against"
            value={selectValue}
            onChange={(e) => {
              const v = e.target.value;
              if (v === "custom") {
                setRefValue(isCustom ? session.baselineSpec : "");
                setRefOpen(true);
              } else {
                setRefOpen(false);
                onSetBaseline(v);
              }
            }}
          >
            <option value="head">HEAD</option>
            <option value="trust-point" disabled={!session.trustPoint}>
              {session.trustPoint ? `Trust point (${session.trustPoint})` : "Trust point — none yet"}
            </option>
            <option value="merge-base">Merge-base</option>
            <option value="custom">{isCustom && !refOpen ? `Ref: ${session.baselineSpec}` : "Custom ref…"}</option>
          </select>
          {refOpen && (
            <input
              className="baseline-ref"
              aria-label="Custom baseline ref"
              placeholder="branch or SHA, then Enter"
              value={refValue}
              autoFocus
              onChange={(e) => setRefValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && refValue.trim()) {
                  setRefOpen(false);
                  onSetBaseline(refValue.trim());
                } else if (e.key === "Escape") {
                  setRefOpen(false);
                }
              }}
              onBlur={() => setRefOpen(false)}
            />
          )}
        </span>
      </div>
      <div className="spacer" />
      <div className={"summary-pill" + (zero ? " calm" : "")}>
        <span className="dot" />
        <span>
          {session.approved ? (
            <>
              <b>Reviewed</b>
              {session.approvedAt ? ` at ${session.approvedAt}` : ""} —{" "}
              {zero
                ? "no open flags"
                : `${session.riskCount} flag${session.riskCount === 1 ? "" : "s"} still open`}
            </>
          ) : zero ? (
            noDrift ? (
              <>
                <b>Clean</b> — no uncommitted changes
              </>
            ) : (
              <>
                <b>No flags</b> in {session.changedFiles} changed file
                {session.changedFiles === 1 ? "" : "s"}
              </>
            )
          ) : (
            <>
              <b>
                {session.riskCount} flag{session.riskCount === 1 ? "" : "s"}
              </b>{" "}
              in{" "}
              <b>
                {session.fileCount} file{session.fileCount === 1 ? "" : "s"}
              </b>
            </>
          )}
        </span>
      </div>
      <div className="toolbar-actions">
        <button
          className="btn"
          onClick={onDismissAll}
          disabled={zero}
          title={zero ? "No active flags to dismiss" : "Dismiss every active flag (persisted for this repo)"}
        >
          Dismiss all
        </button>
        <button
          className={"btn primary" + (session.approved ? " approved" : "")}
          onClick={onToggleApprove}
          disabled={noDrift}
          aria-pressed={session.approved}
          title={
            noDrift
              ? "Nothing to review — the working tree is clean"
              : session.approved
                ? "Reviewed — click to revoke. Clears automatically when the drift changes."
                : "Records that you reviewed this drift. Auto-clears when files change again."
          }
        >
          {session.approved ? <>{Ico.check} Reviewed</> : "Mark reviewed"}
        </button>
      </div>
    </div>
  );
}
