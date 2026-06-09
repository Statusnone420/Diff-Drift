import type { Session } from "../types";
import { Ico } from "../lib/icons";

export function Toolbar({ session, onSwitchRepo }: { session: Session; onSwitchRepo: () => void }) {
  const zero = session.riskCount === 0;
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
          {zero ? (
            session.changedFiles === 0 ? (
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
        <button className="btn">Dismiss all</button>
        <button className="btn primary">Approve session</button>
      </div>
    </div>
  );
}
