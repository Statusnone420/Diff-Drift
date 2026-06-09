import type { RefObject } from "react";
import type { AstNode, FileEntry, Flag } from "../types";
import { Ico } from "../lib/icons";
import { NodeCard } from "./NodeCard";

interface CenterProps {
  file: FileEntry | null;
  changedFiles: number;
  flagsById: Record<string, Flag>;
  activeNodeId: string | null;
  pulseId: string | null;
  onToggleFlag: (flagId: string) => void;
  registerRef: (id: string, el: HTMLDivElement | null) => void;
  scrollRef: RefObject<HTMLDivElement | null>;
}

export function Center({
  file,
  changedFiles,
  flagsById,
  activeNodeId,
  pulseId,
  onToggleFlag,
  registerRef,
  scrollRef,
}: CenterProps) {
  if (!file) {
    const hasUnanalyzedChanges = changedFiles > 0;
    return (
      <div className="col center">
        <div className="center-clean">
          <span className="center-clean-ic">{Ico.shield}</span>
          <div className="center-clean-title">
            {hasUnanalyzedChanges ? "No analyzable drift detected" : "No drift detected"}
          </div>
          <div className="center-clean-sub">
            {hasUnanalyzedChanges
              ? `${changedFiles} changed file${changedFiles === 1 ? "" : "s"} found, but none are TypeScript or TSX files Diff Drift can inspect.`
              : "The working tree is clean — nothing has changed since the last commit."}
          </div>
        </div>
      </div>
    );
  }

  const counts: Record<string, number> = { added: 0, removed: 0, modified: 0 };
  const walk = (ns: AstNode[]) =>
    ns.forEach((n) => {
      if (counts[n.state] !== undefined) counts[n.state]++;
      if (n.children) walk(n.children);
    });
  walk(file.nodes);
  const noChanges = counts.added + counts.removed + counts.modified === 0;

  return (
    <div className="col center">
      <div className="center-head">
        <div className="ch-left">
          <div className="ch-path">
            <span className="dir">{file.dir}</span>
            {file.name}
          </div>
          <div className="ch-sub">
            <span className="lang">{file.lang}</span>
            <span>·</span>
            <span>{file.summary}</span>
          </div>
        </div>
        <div className="legend">
          <span className="lg">
            <span className="sw a" />+{counts.added} added
          </span>
          <span className="lg">
            <span className="sw m" />~{counts.modified} modified
          </span>
          <span className="lg">
            <span className="sw r" />−{counts.removed} removed
          </span>
        </div>
      </div>
      <div className="col-scroll" ref={scrollRef}>
        <div className="tree">
          {noChanges && (
            <div className="empty-note">
              {Ico.shield}Only formatting or whitespace changed — no structural drift in this file.
            </div>
          )}
          <div className="tree-root">
            {file.nodes.map((n) => (
              <NodeCard
                key={n.id}
                node={n}
                flagsById={flagsById}
                activeNodeId={activeNodeId}
                pulseId={pulseId}
                onToggleFlag={onToggleFlag}
                registerRef={registerRef}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
