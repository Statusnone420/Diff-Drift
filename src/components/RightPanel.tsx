import type { Flag } from "../types";
import { Ico, SEV_LABEL } from "../lib/icons";

interface RightPanelProps {
  flags: Flag[];
  activeFlagId: string | null;
  onSelectFlag: (flagId: string) => void;
}

export function RightPanel({ flags, activeFlagId, onSelectFlag }: RightPanelProps) {
  return (
    <div className="col right">
      <div className="rp-head">
        <span className="rp-title">
          {Ico.flag}Risk Flags <span className="tcount">{flags.length}</span>
        </span>
        <span className="rp-sort">severity ↓</span>
      </div>
      <div className="col-scroll">
        {flags.length === 0 ? (
          <div className="rp-empty">
            <span className="rp-empty-ic">{Ico.check}</span>
            <div className="rp-empty-title">No risk flags</div>
            <div className="rp-empty-sub">Nothing suspicious in this drift.</div>
          </div>
        ) : (
          <div className="flag-list">
            {flags.map((fl) => (
              <div
                key={fl.id}
                className={"flag " + fl.severity + (activeFlagId === fl.id ? " active" : "")}
                onClick={() => onSelectFlag(fl.id)}
              >
                <div className="flag-bar" />
                <div className="flag-in">
                  <div className="flag-top">
                    <span className="sev-badge">
                      <span className="ic">{Ico.warn}</span>
                      {SEV_LABEL[fl.severity]}
                    </span>
                    <span className="flag-type">{fl.type}</span>
                  </div>
                  <div className="flag-desc">{fl.desc}</div>
                  <div className="flag-map">
                    <span className="flag-loc">
                      <span className="fp">{fl.filePath}</span>
                    </span>
                    <span className="flag-jump">{Ico.jump} node</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="rp-foot">
        <button className="btn">Export report</button>
        <button className="btn primary">Resolve all</button>
      </div>
    </div>
  );
}
