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
