const { useState, useEffect, useRef, useCallback } = React;
const DATA = window.DRIFT_DATA;

// ---------- small icon helpers ----------
const Ico = {
  file: (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
      <path d="M4 1.5h5L13 5v9.5H4z" stroke="currentColor" strokeWidth="1.2" fill="none"/>
      <path d="M9 1.5V5h4" stroke="currentColor" strokeWidth="1.2" fill="none"/>
    </svg>
  ),
  branch: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <circle cx="4" cy="3.5" r="1.8" stroke="currentColor" strokeWidth="1.2"/>
      <circle cx="4" cy="12.5" r="1.8" stroke="currentColor" strokeWidth="1.2"/>
      <circle cx="12" cy="3.5" r="1.8" stroke="currentColor" strokeWidth="1.2"/>
      <path d="M4 5.3v5.4M12 5.3c0 3-2 4-5 4.2" stroke="currentColor" strokeWidth="1.2" fill="none"/>
    </svg>
  ),
  chevron: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  flag: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <path d="M4 14V2.5M4 3h7l-1.4 2.2L11 7.5H4" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" fill="none"/>
    </svg>
  ),
  warn: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <path d="M8 2l6.2 11H1.8z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" fill="none"/>
      <path d="M8 6.4v3.1" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
      <circle cx="8" cy="11.4" r="0.8" fill="currentColor"/>
    </svg>
  ),
  jump: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path d="M3 8h8M8 4l4 4-4 4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  spark: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <path d="M8 1.5l1.6 4.9L14.5 8l-4.9 1.6L8 14.5l-1.6-4.9L1.5 8l4.9-1.6z" fill="currentColor"/>
    </svg>
  ),
  shield: (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
      <path d="M8 1.6l5 1.8v4.1c0 3.2-2.1 5.4-5 6.9-2.9-1.5-5-3.7-5-6.9V3.4z" stroke="currentColor" strokeWidth="1.2" fill="none"/>
      <path d="M5.7 8.1l1.7 1.7 3-3.4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  check: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path d="M3 8.5l3 3 7-7.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
};

const GLYPH = {
  ImportDeclaration: "im",
  FunctionDeclaration: "fn",
  VariableDeclaration: "let",
  IfStatement: "if",
  ExpressionStatement: "()",
  ReturnStatement: "ret",
  ExportDeclaration: "ex",
};
const SEV_LABEL = { high: "High", medium: "Medium", low: "Low" };
const HL_COLOR = { high: "#f2604c", medium: "#e7a83e", low: "#6f8bc4" };
const HL_COLOR_A = { high: "rgba(242,96,76,0.5)", medium: "rgba(231,168,62,0.5)", low: "rgba(111,139,196,0.5)" };

// ---------- diff block ----------
function DiffLine({ kind, text }) {
  return (
    <div className={"diff-line " + kind}>
      <span className="gutter">{kind === "add" ? "+" : "-"}</span>
      <span className="code">{text}</span>
    </div>
  );
}

function DiffBody({ node }) {
  const hasBefore = node.before && node.before.length;
  const hasAfter = node.after && node.after.length;
  return (
    <div className="node-body">
      <div className="diff">
        <div className="diff-group">
          {hasBefore && node.before.map((l, i) => <DiffLine key={"b" + i} kind="del" text={l} />)}
          {hasBefore && hasAfter && <div className="diff-sep" />}
          {hasAfter && node.after.map((l, i) => <DiffLine key={"a" + i} kind="add" text={l} />)}
        </div>
      </div>
    </div>
  );
}

// ---------- AST node ----------
function NodeCard({ node, flagsById, activeNodeId, pulseId, onToggleFlag, registerRef, defaultOpen }) {
  const changed = node.state !== "unchanged";
  const [open, setOpen] = useState(defaultOpen !== false);
  const flag = node.flagId ? flagsById[node.flagId] : null;
  const isActive = activeNodeId === node.id;
  const isPulse = pulseId === node.id;

  // auto-open when activated
  useEffect(() => { if (isActive && changed) setOpen(true); }, [isActive, changed]);

  const hlStyle = {};
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
  ].join(" ");

  return (
    <div className="node">
      <div
        className={cls}
        style={hlStyle}
        ref={(el) => registerRef(node.id, el)}
      >
        <div className="node-head" onClick={changed ? () => setOpen((o) => !o) : undefined}>
          <span className="node-glyph">{GLYPH[node.kind] || "·"}</span>
          <span className="node-title">
            <span className="row1">
              <span className="node-name">{node.name}</span>
              {node.signature && <span className="node-sig">{node.signature}</span>}
            </span>
            <span className="node-kind">{node.kind}</span>
          </span>

          {flag && (
            <span
              className={"node-flagchip " + flag.severity}
              onClick={(e) => { e.stopPropagation(); onToggleFlag(flag.id); }}
              title={flag.type}
            >
              {Ico.warn}{SEV_LABEL[flag.severity]}
            </span>
          )}
          {changed && <span className={"state-badge " + node.state}>{node.state}</span>}
          {changed && <span className="chev">{Ico.chevron}</span>}
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
              registerRef={registerRef}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ---------- top bar ----------
function TitleBar({ session }) {
  return (
    <div className="titlebar">
      <div className="tb-left">
        <span className="tb-logo">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
            <path d="M2 11.5C4 11.5 4 4.5 8 4.5s4 7 6 7" stroke="#e7a83e" strokeWidth="1.5" strokeLinecap="round" fill="none"/>
            <circle cx="8" cy="4.5" r="1.5" fill="#e7a83e"/>
          </svg>
        </span>
        <span className="tb-title">Drift Inspector</span>
      </div>
      <div className="tb-caption">
        <div className="cap-btn" title="Minimize">
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" strokeWidth="1"/></svg>
        </div>
        <div className="cap-btn" title="Maximize">
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" rx="0.5" stroke="currentColor" strokeWidth="1" fill="none"/></svg>
        </div>
        <div className="cap-btn close" title="Close">
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" strokeWidth="1"/></svg>
        </div>
      </div>
    </div>
  );
}

function Toolbar({ session }) {
  return (
    <div className="toolbar">
      <div className="crumb">
        <span className="proj">{session.project}</span>
        <span className="sep">·</span>
        <span className="branch">{Ico.branch}{session.branch}</span>
      </div>
      <div className="spacer" />
      <div className="summary-pill">
        <span className="dot" />
        <span><b>{session.riskCount} risks</b> across <b>{session.fileCount} files</b></span>
      </div>
      <div className="toolbar-actions">
        <button className="btn">Dismiss all</button>
        <button className="btn primary">Approve session</button>
      </div>
    </div>
  );
}

// ---------- sidebar ----------
function Sidebar({ session, files, selectedId, onSelect }) {
  return (
    <div className="col sidebar">
      <div className="col-scroll">
        <div className="sb-head">
          <div className="sb-title-row">
            <span className="sb-title">Session</span>
            <span className="live-pill"><span className="live-dot" />Active</span>
          </div>
          <div className="sb-meta">
            <div className="meta-cell"><div className="k">Edits</div><div className="v">{session.edits}</div></div>
            <div className="meta-cell"><div className="k">Elapsed</div><div className="v">{session.elapsed}</div></div>
            <div className="meta-cell meta-agent">
              <span className="av">{Ico.spark}</span>
              <div><div className="k">Agent</div><div className="v">{session.agent}</div></div>
            </div>
          </div>
        </div>

        <div className="sb-section-label">
          <span>Files touched</span>
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
      <div className="sb-foot"><span className="gd" />Watching working tree</div>
    </div>
  );
}

// ---------- center ----------
function Center({ file, flagsById, activeNodeId, pulseId, onToggleFlag, registerRef, scrollRef }) {
  const counts = { added: 0, removed: 0, modified: 0 };
  const walk = (ns) => ns.forEach((n) => { if (counts[n.state] !== undefined) counts[n.state]++; if (n.children) walk(n.children); });
  walk(file.nodes);
  const noChanges = counts.added + counts.removed + counts.modified === 0;

  return (
    <div className="col center">
      <div className="center-head">
        <div className="ch-left">
          <div className="ch-path"><span className="dir">{file.dir}</span>{file.name}</div>
          <div className="ch-sub">
            <span className="lang">{file.lang}</span>
            <span>·</span>
            <span>{file.summary}</span>
          </div>
        </div>
        <div className="legend">
          <span className="lg"><span className="sw a" />+{counts.added} added</span>
          <span className="lg"><span className="sw m" />~{counts.modified} modified</span>
          <span className="lg"><span className="sw r" />−{counts.removed} removed</span>
        </div>
      </div>
      <div className="col-scroll" ref={scrollRef}>
        <div className="tree">
          {noChanges && (
            <div className="empty-note">{Ico.shield}No security-relevant structural changes in this file.</div>
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

// ---------- right panel ----------
function RightPanel({ flags, activeFlagId, onSelectFlag }) {
  return (
    <div className="col right">
      <div className="rp-head">
        <span className="rp-title">{Ico.flag}Risk Flags <span className="tcount">{flags.length}</span></span>
        <span className="rp-sort">severity ↓</span>
      </div>
      <div className="col-scroll">
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
                  <span className="sev-badge"><span className="ic">{Ico.warn}</span>{SEV_LABEL[fl.severity]}</span>
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
      </div>
      <div className="rp-foot">
        <button className="btn" style={{ flex: 1, justifyContent: "center" }}>Export report</button>
        <button className="btn primary" style={{ flex: 1, justifyContent: "center" }}>Resolve all</button>
      </div>
    </div>
  );
}

// ---------- app ----------
function App() {
  const { session, files, flags } = DATA;
  const flagsById = {};
  flags.forEach((f) => (flagsById[f.id] = f));

  const [selectedId, setSelectedId] = useState("auth");
  const [activeNodeId, setActiveNodeId] = useState("n_pattern");
  const [activeFlagId, setActiveFlagId] = useState("f1");
  const [pulseId, setPulseId] = useState(null);

  const nodeRefs = useRef({});
  const scrollRef = useRef(null);
  const pulseTimer = useRef(null);

  const registerRef = useCallback((id, el) => {
    if (el) nodeRefs.current[id] = el; 
  }, []);

  const file = files.find((f) => f.id === selectedId) || files[0];

  const scrollToNode = useCallback((nodeId) => {
    requestAnimationFrame(() => {
      const el = nodeRefs.current[nodeId];
      const cont = scrollRef.current;
      if (el && cont) {
        const r = el.getBoundingClientRect();
        const cr = cont.getBoundingClientRect();
        const target = cont.scrollTop + (r.top - cr.top) - 96;
        cont.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
      }
    });
  }, []);

  const firePulse = useCallback((nodeId) => {
    setPulseId(null);
    if (pulseTimer.current) clearTimeout(pulseTimer.current);
    requestAnimationFrame(() => {
      setPulseId(nodeId);
      pulseTimer.current = setTimeout(() => setPulseId(null), 720);
    });
  }, []);

  // select a flag -> switch file if needed, highlight + scroll to node
  const selectFlag = useCallback((flagId) => {
    const fl = flagsById[flagId];
    if (!fl) return;
    setActiveFlagId(flagId);
    setActiveNodeId(fl.nodeId);
    if (fl.fileId !== selectedId) {
      setSelectedId(fl.fileId);
      setTimeout(() => { scrollToNode(fl.nodeId); firePulse(fl.nodeId); }, 90);
    } else {
      scrollToNode(fl.nodeId);
      firePulse(fl.nodeId);
    }
  }, [flagsById, selectedId, scrollToNode, firePulse]);

  // toggle flag chip on a node -> activates that flag (reverse tie)
  const toggleFlagFromNode = useCallback((flagId) => {
    selectFlag(flagId);
  }, [selectFlag]);

  const selectFile = useCallback((fileId) => {
    setSelectedId(fileId);
    setActiveNodeId(null);
    setActiveFlagId(null);
  }, []);

  // initial scroll to default active node
  useEffect(() => { if (activeNodeId) scrollToNode(activeNodeId); }, []); // eslint-disable-line

  return (
    <div className="desktop">
      <div className="window">
        <TitleBar session={session} />
        <Toolbar session={session} />
        <div className="body">
          <Sidebar session={session} files={files} selectedId={selectedId} onSelect={selectFile} />
          <Center
            file={file}
            flagsById={flagsById}
            activeNodeId={activeNodeId}
            pulseId={pulseId}
            onToggleFlag={toggleFlagFromNode}
            registerRef={registerRef}
            scrollRef={scrollRef}
          />
          <RightPanel flags={flags} activeFlagId={activeFlagId} onSelectFlag={selectFlag} />
        </div>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
