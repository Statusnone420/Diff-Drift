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
            <div
              key={f.id}
              className={"file-row" + (f.id === selectedId ? " sel" : "")}
              onClick={() => onSelect(f.id)}
            >
              <span className="file-ic">{Ico.file}</span>
              <span className="file-main">
                <div className="file-name">{f.name}</div>
                <div className="file-dir">{f.dir}</div>
              </span>
              <span className={"file-badge r" + (f.risks > 1 ? 2 : f.risks)}>{f.risks}</span>
            </div>
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
