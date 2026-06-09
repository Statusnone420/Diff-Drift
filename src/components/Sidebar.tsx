import type { FileEntry, Session } from "../types";
import { Ico } from "../lib/icons";

interface SidebarProps {
  session: Session;
  files: FileEntry[];
  selectedId: string | null;
  onSelect: (fileId: string) => void;
  watchingSince: string | null;
  justUpdated: boolean;
}

export function Sidebar({
  session,
  files,
  selectedId,
  onSelect,
  watchingSince,
  justUpdated,
}: SidebarProps) {
  return (
    <div className="col sidebar">
      <div className="col-scroll">
        <div className="sb-head">
          <div className="sb-title-row">
            <span className="sb-title">Session</span>
            <span className="live-pill">
              <span className="live-dot" />
              Active
            </span>
          </div>
          <div className="sb-meta">
            <div className="meta-cell">
              <div className="k">Changed</div>
              <div className="v">{session.changedFiles}</div>
            </div>
            <div className="meta-cell">
              <div className="k">Risks</div>
              <div className="v">{session.riskCount}</div>
            </div>
            <div className="meta-cell meta-agent">
              <span className="av">{Ico.eye}</span>
              <div>
                <div className="k">Watching</div>
                <div className="v">{watchingSince ? `since ${watchingSince}` : "live"}</div>
              </div>
            </div>
          </div>
        </div>

        <div className="sb-section-label">
          <span>Files analyzed</span>
          <span className="count">{files.length}</span>
        </div>
        <div className="file-list">
          {files.map((f) => (
            <button
              key={f.id}
              className={"file-row" + (f.id === selectedId ? " sel" : "")}
              onClick={() => onSelect(f.id)}
              aria-current={f.id === selectedId || undefined}
              aria-label={`${f.dir}${f.name}, ${f.risks} risk${f.risks === 1 ? "" : "s"}`}
            >
              <span className="file-ic">{Ico.file}</span>
              <span className="file-main">
                <span className="file-name">{f.name}</span>
                <span className="file-dir">{f.dir}</span>
              </span>
              <span className={"file-badge r" + (f.risks > 1 ? 2 : f.risks)}>{f.risks}</span>
            </button>
          ))}
        </div>
      </div>
      <div className={"sb-foot" + (justUpdated ? " updated" : "")}>
        <span className="gd" />
        {justUpdated ? "Updated just now" : "Watching working tree"}
      </div>
    </div>
  );
}
