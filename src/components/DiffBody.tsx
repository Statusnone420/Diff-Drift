import type { AstNode } from "../types";

function DiffLine({ kind, text }: { kind: "add" | "del"; text: string }) {
  return (
    <div className={"diff-line " + kind}>
      <span className="gutter">{kind === "add" ? "+" : "-"}</span>
      <span className="code">{text}</span>
    </div>
  );
}

export function DiffBody({ node }: { node: AstNode }) {
  const hasBefore = !!(node.before && node.before.length);
  const hasAfter = !!(node.after && node.after.length);
  return (
    <div className="node-body">
      <div className="diff">
        <div className="diff-group">
          {hasBefore && node.before!.map((l, i) => <DiffLine key={"b" + i} kind="del" text={l} />)}
          {hasBefore && hasAfter && <div className="diff-sep" />}
          {hasAfter && node.after!.map((l, i) => <DiffLine key={"a" + i} kind="add" text={l} />)}
        </div>
      </div>
    </div>
  );
}
