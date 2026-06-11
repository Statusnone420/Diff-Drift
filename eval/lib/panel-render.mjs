// Renders the multi-model panel as a single high-end benchmark leaderboard.
// Self-contained HTML (no external assets) so scripts/capture-scorecard.mjs can
// screenshot the <main> element straight to a PNG.

const PENDING = [
  { key: "gpt-5-5", label: "GPT-5.5", vendor: "OpenAI" },
  { key: "gemini", label: "Gemini", vendor: "Google" },
];

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}

function band(score) {
  if (score == null) return "none";
  if (score === 100) return "good";
  if (score >= 80) return "warn";
  return "bad";
}

function heatcells(cases, perCase) {
  return cases
    .map((c) => {
      const s = perCase?.[c.caseId]?.score ?? null;
      const label = s == null ? "—" : s;
      return `<span class="cell ${band(s)}" title="${esc(c.title)} — ${label}/100"></span>`;
    })
    .join("");
}

export function renderPanelHtml(r) {
  const date = (r.generatedAt || "").slice(0, 10);
  const present = new Set(r.models.map((m) => m.key));
  const pendingRows = PENDING.filter((p) => !present.has(p.key));
  const leader = r.models[0];

  const modelRows = r.models
    .map((m, i) => {
      const rank = i + 1;
      return `
      <div class="row">
        <div class="rank">${rank}</div>
        <div class="who">
          <div class="name">${esc(m.label)}</div>
          <div class="vendor">${esc(m.vendor)}</div>
        </div>
        <div class="score">
          <div class="num ${band(m.total)}">${m.total}<span class="den">/100</span></div>
          <div class="bar"><i class="${band(m.total)}" style="width:${m.total}%"></i></div>
        </div>
        <div class="stat"><span>${Math.round(m.decisionAccuracy * 100)}%</span><label>decision</label></div>
        <div class="stat"><span>${Math.round(m.recall * 100)}%</span><label>recall</label></div>
        <div class="heat" aria-label="per-case scores">${heatcells(r.cases, m.perCase)}</div>
      </div>`;
    })
    .join("");

  const pendingHtml = pendingRows
    .map(
      (p) => `
      <div class="row pending">
        <div class="rank">·</div>
        <div class="who">
          <div class="name">${esc(p.label)}</div>
          <div class="vendor">${esc(p.vendor)}</div>
        </div>
        <div class="score"><div class="num muted">—</div><div class="bar"><i style="width:0"></i></div></div>
        <div class="stat"><span>—</span><label>decision</label></div>
        <div class="stat"><span>—</span><label>recall</label></div>
        <div class="heat awaiting">awaiting a blind run · drop answers in panel/${esc(p.key)}/</div>
      </div>`,
    )
    .join("");

  const signalNote = r.productSignals?.length
    ? `<strong>${r.productSignals.length}</strong> case(s) every model misses — a real clarity gap worth fixing: <code>${r.productSignals
        .map(esc)
        .join("</code> <code>")}</code>`
    : `No case is missed by every model — detection clarity holds across the panel.`;

  return `<!-- Diff Drift blind-agent multi-model panel -->
<style>
  :root {
    --bg: #0a0d13; --panel: #121726; --panel2: #0e1320; --line: #232a3d;
    --ink: #eef1f7; --muted: #8a93a8; --faint: #5b6479;
    --good: #46c46a; --warn: #e7a83e; --bad: #f2604c; --accent: #6f8bc4; --teal: #24c8db;
    --mono: ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace;
    --sans: "Inter", "Segoe UI", system-ui, -apple-system, sans-serif;
  }
  * { box-sizing: border-box; }
  main {
    width: 1180px; margin: 0 auto; padding: 40px 44px 34px;
    background: radial-gradient(1200px 500px at 78% -8%, #16203a 0%, transparent 60%), var(--bg);
    color: var(--ink); font-family: var(--sans); -webkit-font-smoothing: antialiased;
    border: 1px solid var(--line); border-radius: 18px;
  }
  .head { display: flex; justify-content: space-between; align-items: flex-end; gap: 24px; padding-bottom: 22px; border-bottom: 1px solid var(--line); }
  .eyebrow { font: 600 12px/1 var(--sans); letter-spacing: .18em; text-transform: uppercase; color: var(--teal); margin-bottom: 12px; }
  h1 { font: 700 32px/1.05 var(--sans); margin: 0 0 10px; letter-spacing: -0.01em; }
  .sub { color: var(--muted); font-size: 14.5px; line-height: 1.5; max-width: 620px; margin: 0; }
  .spread { text-align: right; flex: none; }
  .spread .tag { font: 600 11px/1 var(--sans); letter-spacing: .16em; text-transform: uppercase; color: var(--faint); margin-bottom: 8px; }
  .spread .big { font: 700 46px/1 var(--mono); letter-spacing: -0.02em; color: var(--ink); white-space: nowrap; }
  .spread .big small { font-size: 30px; color: var(--teal); font-weight: 700; margin: 0 6px; vertical-align: 4px; }
  .spread .cap { color: var(--muted); font-size: 12.5px; margin-top: 8px; }
  .spread .lead { color: var(--good); font-weight: 600; }

  .board { margin-top: 18px; display: flex; flex-direction: column; gap: 8px; }
  .colhead, .row { display: grid; grid-template-columns: 40px 200px 150px 92px 92px 1fr; align-items: center; gap: 16px; }
  .colhead { padding: 4px 16px 10px; color: var(--faint); font: 600 11px/1 var(--sans); letter-spacing: .08em; text-transform: uppercase; }
  .colhead .h-heat { text-align: left; }
  .row { background: linear-gradient(180deg, var(--panel) 0%, var(--panel2) 100%); border: 1px solid var(--line); border-radius: 12px; padding: 16px; }
  .row.pending { opacity: .55; background: none; border-style: dashed; }
  .rank { font: 700 18px/1 var(--mono); color: var(--faint); text-align: center; }
  .who .name { font: 600 16px/1.2 var(--sans); }
  .who .vendor { color: var(--muted); font-size: 12.5px; margin-top: 3px; }
  .score .num { font: 700 30px/1 var(--mono); letter-spacing: -0.02em; }
  .score .num .den { font-size: 14px; color: var(--faint); font-weight: 600; margin-left: 2px; }
  .score .num.good { color: var(--good); } .score .num.warn { color: var(--warn); } .score .num.bad { color: var(--bad); } .score .num.muted { color: var(--faint); }
  .score .bar { height: 5px; border-radius: 3px; background: #1c2336; margin-top: 8px; overflow: hidden; }
  .score .bar i { display: block; height: 100%; border-radius: 3px; background: var(--accent); }
  .score .bar i.good { background: var(--good); } .score .bar i.warn { background: var(--warn); } .score .bar i.bad { background: var(--bad); }
  .stat { text-align: center; } .stat span { font: 600 17px/1 var(--mono); color: var(--ink); } .stat label { display: block; color: var(--faint); font-size: 11px; margin-top: 5px; letter-spacing: .04em; }
  .heat { display: flex; gap: 4px; flex-wrap: nowrap; }
  .heat .cell { width: 100%; height: 26px; border-radius: 4px; flex: 1 1 0; }
  .cell.good { background: linear-gradient(180deg, #2f9c52, #246e3d); }
  .cell.warn { background: linear-gradient(180deg, #d99a35, #a9762450); box-shadow: inset 0 0 0 1px #e7a83e88; }
  .cell.bad { background: linear-gradient(180deg, #e0503d, #9c2f24); box-shadow: inset 0 0 0 1px #f2604c; }
  .cell.none { background: #1a2030; }
  .heat.awaiting { color: var(--faint); font-size: 12.5px; font-style: italic; align-items: center; }

  .foot { margin-top: 24px; padding-top: 18px; border-top: 1px solid var(--line); display: flex; justify-content: space-between; gap: 24px; align-items: flex-start; }
  .legend { display: flex; gap: 18px; align-items: center; color: var(--muted); font-size: 12.5px; }
  .legend i { display: inline-block; width: 12px; height: 12px; border-radius: 3px; margin-right: 6px; vertical-align: -2px; }
  .legend i.good { background: #2f9c52; } .legend i.warn { background: #d99a35; } .legend i.bad { background: #e0503d; }
  .banner { color: var(--warn); font-size: 12.5px; font-weight: 600; }
  .banner span { color: var(--muted); font-weight: 400; }
  .signal { margin-top: 16px; background: #11192a; border: 1px solid var(--line); border-left: 3px solid var(--teal); border-radius: 8px; padding: 12px 16px; color: var(--muted); font-size: 13px; line-height: 1.5; }
  .signal code { font-family: var(--mono); font-size: 12px; color: var(--ink); background: #1a2236; padding: 1px 6px; border-radius: 4px; }
  .meta { color: var(--faint); font-size: 12px; margin-top: 10px; }
</style>
<main>
  <div class="head">
    <div>
      <div class="eyebrow">Diff Drift · blind-agent benchmark</div>
      <h1>One system, many reviewers</h1>
      <p class="sub">Each model plays the blind reviewer over the same benchmark v4 packets and scores how reliably it reaches the right trust decision from Diff Drift's output. The models are rulers; Diff Drift is what's measured — read the spread, not a pooled average.</p>
    </div>
    <div class="spread">
      <div class="tag">Score range</div>
      <div class="big">${r.spread.min}<small>to</small>${r.spread.max}</div>
      <div class="cap">/100 · ${r.models.length} models${leader ? ` · <span class="lead">${esc(leader.label)} leads</span>` : ""}</div>
    </div>
  </div>

  <div class="colhead">
    <div>#</div><div>Model</div><div>Overall</div><div>Decision</div><div>Recall</div><div class="h-heat">Per-case (${r.cases.length} tests · hardest first)</div>
  </div>
  <div class="board">
    ${modelRows}
    ${pendingHtml}
  </div>

  <div class="signal">${signalNote}</div>

  <div class="foot">
    <div class="legend">
      <span><i class="good"></i>100</span>
      <span><i class="warn"></i>80–99</span>
      <span><i class="bad"></i>&lt;80</span>
    </div>
    <div>
      <div class="banner">Independent external validation pending. <span>All evaluators are models inside the project; clearing this needs a human reviewer outside it.</span></div>
      <div class="meta">Benchmark v4 · same frozen rubric &amp; packets across every model · generated ${esc(date)}</div>
    </div>
  </div>
</main>
`;
}
