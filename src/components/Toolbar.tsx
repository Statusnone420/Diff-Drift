import type { Session } from "../types";
import { Ico } from "../lib/icons";

interface ToolbarProps {
  session: Session;
  onSwitchRepo: () => void;
  onDismissAll: () => void;
  onToggleApprove: () => void;
}

export function Toolbar({ session, onSwitchRepo, onDismissAll, onToggleApprove }: ToolbarProps) {
  const zero = session.riskCount === 0;
  const noDrift = session.changedFiles === 0;
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
      </div>
      <div className="spacer" />
      <div className={"summary-pill" + (zero || session.approved ? " calm" : "")}>
        <span className="dot" />
        <span>
          {session.approved ? (
            <>
              <b>Approved</b>
              {session.approvedAt ? ` at ${session.approvedAt}` : ""} —{" "}
              {zero ? "no active risks" : `${session.riskCount} active risks`}
            </>
          ) : zero ? (
            noDrift ? (
              <>
                <b>Clean</b> — no uncommitted changes
              </>
            ) : (
              <>
                <b>No risks</b> in {session.changedFiles} changed files
              </>
            )
          ) : (
            <>
              <b>{session.riskCount} risks</b> across <b>{session.fileCount} files</b>
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
              ? "Nothing to approve — the working tree is clean"
              : session.approved
                ? "Approved — click to revoke. Approval auto-revokes when the drift changes."
                : "Mark this drift as reviewed and approved"
          }
        >
          {session.approved ? <>{Ico.check} Approved</> : "Approve session"}
        </button>
      </div>
    </div>
  );
}
