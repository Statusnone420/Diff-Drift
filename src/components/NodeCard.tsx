import { useEffect, useState, type CSSProperties } from "react";
import type { AstNode, Flag } from "../types";
import { Ico, GLYPH, SEV_LABEL, HL_COLOR, HL_COLOR_A } from "../lib/icons";
import { DiffBody } from "./DiffBody";

interface NodeCardProps {
  node: AstNode;
  flagsById: Record<string, Flag>;
  activeNodeId: string | null;
  pulseId: string | null;
  onToggleFlag: (flagId: string) => void;
  onToggleReviewed: (nodeId: string, reviewed: boolean) => void;
  registerRef: (id: string, el: HTMLDivElement | null) => void;
  defaultOpen?: boolean;
}

export function NodeCard({
  node,
  flagsById,
  activeNodeId,
  pulseId,
  onToggleFlag,
  onToggleReviewed,
  registerRef,
  defaultOpen,
}: NodeCardProps) {
  const changed = node.state !== "unchanged";
  const [open, setOpen] = useState(defaultOpen !== false);
  const flag = node.flagId ? flagsById[node.flagId] : null;
  const isActive = activeNodeId === node.id;
  const isPulse = pulseId === node.id;

  // auto-open when activated
  useEffect(() => {
    if (isActive && changed) setOpen(true);
  }, [isActive, changed]);

  const hlStyle: CSSProperties & Record<string, string> = {};
  if (isActive) {
    const sev = flag ? flag.severity : "medium";
    hlStyle["--hl"] = HL_COLOR[sev];
    hlStyle["--hlA"] = HL_COLOR_A[sev];
  }

  const cls = [
    "node-card",
    "state-" + node.state,
    changed ? "changed" : "",
    open ? "open" : "",
    isActive ? "is-active" : "",
    isPulse ? "pulse" : "",
    changed && node.reviewed ? "reviewed" : "",
  ].join(" ");

  const toggleOpen = () => setOpen((o) => !o);
  const headerContent = (
    <>
      <span className="node-glyph">{GLYPH[node.kind] || "·"}</span>
      <span className="node-title">
        <span className="row1">
          <span className="node-name">{node.name}</span>
          {node.signature && <span className="node-sig">{node.signature}</span>}
        </span>
        <span className="node-kind">{node.kind}</span>
      </span>
      {changed && <span className={"state-badge " + node.state}>{node.state}</span>}
      {changed && <span className="chev">{Ico.chevron}</span>}
    </>
  );

  return (
    <div className="node">
      <div className={cls} style={hlStyle} ref={(el) => registerRef(node.id, el)}>
        <div className="node-head">
          {changed ? (
            <button
              type="button"
              className="node-main"
              onClick={toggleOpen}
              aria-expanded={open}
              aria-label={`${node.kind} ${node.name}: ${node.state}`}
            >
              {headerContent}
            </button>
          ) : (
            <div className="node-main">{headerContent}</div>
          )}
          {flag && (
            <button
              className={"node-flagchip " + flag.severity + (flag.dismissed ? " muted" : "")}
              onClick={(e) => {
                e.stopPropagation();
                onToggleFlag(flag.id);
              }}
              title={flag.dismissed ? `${flag.type} (dismissed)` : flag.type}
              aria-label={`Show flag: ${flag.type}`}
            >
              {Ico.warn}
              {SEV_LABEL[flag.severity]}
            </button>
          )}
          {changed && (
            <button
              className={"node-review" + (node.reviewed ? " on" : "")}
              onClick={(e) => {
                e.stopPropagation();
                onToggleReviewed(node.id, !node.reviewed);
              }}
              aria-pressed={node.reviewed}
              aria-label={`${node.reviewed ? "Mark unreviewed" : "Mark reviewed"}: ${node.kind} ${node.name}`}
              title={
                node.reviewed
                  ? "Reviewed — clears automatically if this change drifts again"
                  : "Mark this change reviewed"
              }
            >
              {Ico.check}
            </button>
          )}
        </div>

        {changed && open && <DiffBody node={node} />}
      </div>

      {node.children && node.children.length > 0 && (
        <div className="node-children">
          {node.children.map((c) => (
            <NodeCard
              key={c.id}
              node={c}
              flagsById={flagsById}
              activeNodeId={activeNodeId}
              pulseId={pulseId}
              onToggleFlag={onToggleFlag}
              onToggleReviewed={onToggleReviewed}
              registerRef={registerRef}
            />
          ))}
        </div>
      )}
    </div>
  );
}
