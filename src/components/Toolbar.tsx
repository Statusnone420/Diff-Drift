import { useEffect, useRef, useState } from "react";
import type { Session } from "../types";
import { baselinePhrase } from "../lib/baseline";
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
  const [scopeOpen, setScopeOpen] = useState(false);
  const [customOpen, setCustomOpen] = useState(false);
  const [refValue, setRefValue] = useState("");
  const scopeRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    if (!scopeOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!scopeRef.current?.contains(event.target as Node)) {
        setScopeOpen(false);
        setCustomOpen(false);
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [scopeOpen]);

  const scopeTitle = isCustom
    ? `Custom: ${session.baselineSpec}`
    : session.baselineSpec === "trust-point"
      ? "Since last review"
      : session.baselineSpec === "merge-base"
        ? "Entire branch"
        : "Current work";

  const scopeDescription = isCustom
    ? baselinePhrase(session)
    : session.baselineSpec === "trust-point"
      ? "Agent commits stay visible"
      : session.baselineSpec === "merge-base"
        ? "Everything this branch adds"
        : "Uncommitted changes";

  const applyScope = (spec: string) => {
    setScopeOpen(false);
    setCustomOpen(false);
    onSetBaseline(spec);
  };

  const openCustom = () => {
    setRefValue(isCustom ? session.baselineSpec : "");
    setCustomOpen(true);
  };

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
        <span
          ref={scopeRef}
          className="baseline"
          title={`Compare scope: ${baselinePhrase(session)}. Use this when an agent commits as it works, or when you want to review the whole branch.`}
        >
          <button
            type="button"
            className="scope-trigger"
            data-testid="scope-trigger"
            aria-haspopup="dialog"
            aria-expanded={scopeOpen}
            onClick={() => {
              setScopeOpen((open) => !open);
              setCustomOpen(false);
            }}
          >
            <span className="scope-label">Scope</span>
            <span className="scope-value">
              <b>{scopeTitle}</b>
              <span>{scopeDescription}</span>
            </span>
            <span className="scope-chevron" aria-hidden="true">
              {Ico.chevron}
            </span>
          </button>
          {scopeOpen && (
            <div
              className="scope-popover"
              role="dialog"
              aria-label="Analysis scope"
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setScopeOpen(false);
                  setCustomOpen(false);
                }
              }}
            >
              <div className="scope-popover-head">
                <b>Analysis scope</b>
                <span>Choose what the current working tree is compared against.</span>
              </div>
              <button
                type="button"
                className={"scope-option" + (session.baselineSpec === "head" ? " active" : "")}
                onClick={() => applyScope("head")}
              >
                <span>
                  <b>Current work</b>
                  <em>Uncommitted changes since the last commit.</em>
                </span>
                {!session.trustPoint && <i>Default</i>}
              </button>
              <button
                type="button"
                className={"scope-option" + (session.baselineSpec === "trust-point" ? " active" : "")}
                disabled={!session.trustPoint}
                onClick={() => applyScope("trust-point")}
              >
                <span>
                  <b>Since last review</b>
                  <em>
                    {session.trustPoint
                      ? `Keeps drift visible after agent commits. Trust point ${session.trustPoint}.`
                      : "Available after you mark a drift reviewed."}
                  </em>
                </span>
                {session.trustPoint && <i>Agent-safe</i>}
              </button>
              <button
                type="button"
                className={"scope-option" + (session.baselineSpec === "merge-base" ? " active" : "")}
                onClick={() => applyScope("merge-base")}
              >
                <span>
                  <b>Entire branch</b>
                  <em>Everything this branch adds over the default branch.</em>
                </span>
                <i>PR check</i>
              </button>
              <button type="button" className={"scope-option" + (isCustom ? " active" : "")} onClick={openCustom}>
                <span>
                  <b>Custom ref</b>
                  <em>Compare against a branch, tag, or SHA.</em>
                </span>
              </button>
              {customOpen && (
                <div className="scope-custom">
                  <input
                    className="baseline-ref"
                    aria-label="Custom baseline ref"
                    placeholder="branch, tag, or SHA"
                    value={refValue}
                    autoFocus
                    onChange={(e) => setRefValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && refValue.trim()) {
                        applyScope(refValue.trim());
                      } else if (e.key === "Escape") {
                        setCustomOpen(false);
                      }
                    }}
                  />
                  <button type="button" className="btn tiny" disabled={!refValue.trim()} onClick={() => applyScope(refValue.trim())}>
                    Apply
                  </button>
                </div>
              )}
            </div>
          )}
        </span>
      </div>
      <div className="spacer" />
      {session.changedNodes > 0 && (
        <span
          className={"review-progress" + (session.reviewedNodes === session.changedNodes ? " done" : "")}
          title="Changed nodes marked reviewed across the whole drift"
        >
          {session.reviewedNodes}/{session.changedNodes} reviewed
        </span>
      )}
      <div className={"summary-pill" + (zero ? " calm" : "")} data-testid="summary-pill">
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
                <b>Clean</b> — no changes since {baselinePhrase(session)}
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
              ? `Nothing to review — no changes since ${baselinePhrase(session)}`
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
