import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { projectRoot } from "./cases.mjs";
import { gitDiff } from "./repo.mjs";
import { runDiffDrift } from "./cli.mjs";

export const evalOutputRoot = join(projectRoot, ".eval");

export function writeEngineResult(result) {
  const dir = join(evalOutputRoot, "results", "engine");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "latest.json"), `${JSON.stringify(result, null, 2)}\n`);
}

export function writeAgentScores(result) {
  const dir = join(evalOutputRoot, "results", "agents");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "latest.json"), `${JSON.stringify(result, null, 2)}\n`);
  writeFileSync(join(dir, "latest.md"), renderAgentScorecard(result));
  writeFileSync(join(dir, "latest.html"), renderAgentDashboard(result));
}

export function renderAgentScorecard(result) {
  const summary = result.summary ?? {};
  const lines = [
    "# Diff Drift blind-agent scorecard",
    "",
    `Generated: ${result.generatedAt}`,
    "",
    "> Advisory only: this score is not a CI gate. The CI blocker is `npm run eval:engine`; blind-agent scoring measures whether reviewers can use Diff Drift packets to reach the right evidence and decision.",
    "",
    `Overall score: ${bar(result.averageScore)} ${result.averageScore}/100`,
    "",
    `- Decision accuracy: ${percent(summary.decisionAccuracy)}`,
    `- Finding recall: ${percent(summary.averageRecall)}`,
    `- Localization: ${percent(summary.averageLocalization)}`,
    "",
    "| Case | Score | Decision | Recall | Notes |",
    "| --- | ---: | --- | ---: | --- |",
  ];

  for (const score of sortedScores(result.scores)) {
    lines.push(
      `| ${score.caseId} | ${score.score} | ${score.decisionAccepted ? "ok" : "miss"} (${score.acceptedDecisions.join("/")}) | ${percent(
        score.recall,
      )} | ${notesFor(score)} |`,
    );
  }

  lines.push(
    "",
    "## How to improve this score without cheating",
    "",
    "- Improve Diff Drift output so blind reviewers find the same risky nodes with less ambiguity.",
    "- Add harder cases and keep benign cases in the mix so the score cannot rise by always blocking.",
    "- Treat scorer changes as rubric calibration: aliases and accepted decisions should reflect defensible human review, not hide misses.",
    "- Review misses in `missedExpectations` and unmatched findings before changing the product or rubric.",
    "",
  );
  return `${lines.join("\n")}\n`;
}

export function renderAgentDashboard(result) {
  const summary = result.summary ?? {};
  const score = clampScore(result.averageScore);
  const rows = sortedScores(result.scores).map(renderCaseCard).join("\n");
  const dots = sortedScores(result.scores).map(renderScoreDot).join("\n");
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Diff Drift Blind-Agent Scorecard</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #0b0c0f;
      --panel: #101116;
      --panel-2: #16181f;
      --border: #20232b;
      --border-strong: #2c303a;
      --text: #e9ebef;
      --dim: #a5a9b3;
      --mute: #8f95a3;
      --accent: #e7a83e;
      --green: #4ec46a;
      --red: #f2604c;
      --blue: #6f8bc4;
      --mono: "Cascadia Code", "JetBrains Mono", ui-monospace, Consolas, monospace;
      --ui: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background:
        linear-gradient(180deg, rgba(231, 168, 62, 0.035), transparent 280px),
        var(--bg);
      color: var(--text);
      font-family: var(--ui);
      font-size: 14px;
      line-height: 1.55;
    }
    main { max-width: 1160px; margin: 0 auto; padding: 34px 22px 44px; }
    .topline {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 18px;
      margin-bottom: 28px;
      color: var(--dim);
      font-size: 12px;
    }
    .brand { display: flex; align-items: center; gap: 10px; color: var(--text); font-weight: 700; }
    .mark {
      width: 24px;
      height: 24px;
      border-radius: 7px;
      background: linear-gradient(145deg, #2a2d37, #15171d);
      border: 1px solid var(--border-strong);
      position: relative;
    }
    .mark:after {
      content: "";
      position: absolute;
      inset: 7px 6px;
      border-top: 2px solid var(--accent);
      border-left: 2px solid var(--accent);
      transform: rotate(45deg);
    }
    .brief {
      border-top: 1px solid var(--border-strong);
      border-bottom: 1px solid var(--border);
      padding: 28px 0 26px;
      display: grid;
      grid-template-columns: minmax(0, 1fr) 380px;
      gap: 42px;
      align-items: end;
      margin-bottom: 26px;
    }
    h1 {
      margin: 0 0 14px;
      font-size: clamp(34px, 4.4vw, 64px);
      line-height: 1;
      letter-spacing: -0.035em;
      text-wrap: balance;
    }
    .lede { max-width: 70ch; margin: 0; color: var(--dim); font-size: 15px; }
    .score-summary { display: grid; gap: 14px; }
    .score-number { display: flex; align-items: baseline; gap: 10px; justify-content: flex-end; }
    .score-number strong { font-size: 74px; line-height: 0.9; letter-spacing: -0.05em; }
    .score-number span { color: var(--dim); font-family: var(--mono); }
    .score-track { height: 12px; border-radius: 999px; background: #242730; overflow: hidden; }
    .score-track i { display: block; height: 100%; width: ${score}%; background: var(--accent); }
    .metrics { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
    .metric { border-top: 1px solid var(--border); padding-top: 9px; }
    .metric b { display: block; font-size: 20px; letter-spacing: -0.02em; }
    .metric span { color: var(--dim); font-size: 12px; }
    .distribution {
      border: 1px solid var(--border);
      border-radius: 12px;
      background: rgba(16, 17, 22, 0.78);
      padding: 16px;
      margin-bottom: 24px;
    }
    .axis {
      position: relative;
      height: 72px;
      border-bottom: 1px solid var(--border-strong);
      margin: 8px 8px 0;
    }
    .axis:before, .axis:after {
      content: "";
      position: absolute;
      bottom: -4px;
      width: 1px;
      height: 8px;
      background: var(--border-strong);
    }
    .axis:before { left: 0; }
    .axis:after { right: 0; }
    .dot {
      position: absolute;
      left: var(--x);
      top: var(--y);
      width: 11px;
      height: 11px;
      margin-left: -5px;
      border-radius: 50%;
      background: var(--accent);
      border: 2px solid var(--bg);
    }
    .axis-labels { display: flex; justify-content: space-between; color: var(--mute); font: 11px var(--mono); margin: 8px 8px 0; }
    .section-head {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 12px;
      margin: 0 0 12px;
    }
    h2 { margin: 0; font-size: 18px; letter-spacing: -0.01em; }
    .hint { color: var(--mute); font-size: 12px; }
    .cases { display: grid; gap: 12px; }
    .case {
      border: 1px solid var(--border);
      border-radius: 10px;
      background: rgba(16, 17, 22, 0.86);
      padding: 14px 16px;
      display: grid;
      grid-template-columns: 250px minmax(0, 1fr) 250px;
      gap: 18px;
      align-items: center;
    }
    .case-title { font-weight: 700; margin-bottom: 5px; overflow-wrap: anywhere; }
    .case-meta { color: var(--dim); font-size: 12px; font-family: var(--mono); }
    .barline { display: grid; gap: 7px; }
    .bar-label { display: flex; justify-content: space-between; color: var(--dim); font-size: 12px; }
    .bar {
      height: 8px;
      border-radius: 999px;
      background: #23262f;
      overflow: hidden;
    }
    .bar > i {
      display: block;
      height: 100%;
      width: var(--value);
      background: var(--fill);
    }
    .chips { display: flex; flex-wrap: wrap; gap: 7px; }
    .chip {
      border: 1px solid var(--border-strong);
      border-radius: 999px;
      padding: 3px 8px;
      font-size: 12px;
      color: var(--dim);
      white-space: nowrap;
    }
    .chip.ok { color: #9ce0aa; border-color: rgba(78, 196, 106, 0.35); background: rgba(78, 196, 106, 0.08); }
    .chip.miss { color: #f49c8e; border-color: rgba(242, 96, 76, 0.35); background: rgba(242, 96, 76, 0.08); }
    .why { color: var(--dim); font-size: 12px; margin: 10px 0 0; }
    .why b { color: var(--text); }
    .callout {
      margin-top: 20px;
      border-top: 1px solid var(--border);
      padding-top: 16px;
      color: #c5d3ef;
    }
    .callout h2 { margin-bottom: 8px; }
    .callout ul { margin: 0; padding-left: 18px; }
    @media (max-width: 900px) {
      .hero, .case { grid-template-columns: 1fr; }
      .score-ring { width: 150px; }
      .metrics { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <main>
    <div class="topline">
      <div class="brand"><span class="mark"></span><span>Diff Drift benchmark</span></div>
      <div>${escapeHtml(result.generatedAt)}</div>
    </div>
    <section class="brief">
      <div>
        <h1>Blind-agent scorecard</h1>
        <p class="lede">This advisory benchmark checks whether reviewers can use Diff Drift packets to make the right trust decision and cite the right evidence. It is designed to improve the product, not to block releases.</p>
      </div>
      <aside class="score-summary" aria-label="Overall score">
        <div class="score-number"><strong>${score}</strong><span>/100</span></div>
        <div class="score-track" aria-hidden="true"><i></i></div>
        <div class="metrics">
          ${metric("Decision", summary.decisionAccuracy)}
          ${metric("Recall", summary.averageRecall)}
          ${metric("Location", summary.averageLocalization)}
        </div>
      </aside>
    </section>
    <section class="distribution" aria-label="Case score distribution">
      <div class="section-head">
        <h2>Score distribution</h2>
        <div class="hint">Each point is one blind-review packet.</div>
      </div>
      <div class="axis">${dots}</div>
      <div class="axis-labels"><span>0</span><span>50</span><span>100</span></div>
    </section>
    <div class="section-head">
      <h2>Case results</h2>
      <div class="hint">Scores combine weighted finding recall, decision accuracy, top-risk ranking, localization, and unmatched findings.</div>
    </div>
    <section class="cases">${rows}</section>
    <section class="callout">
      <h2>How this gets to 95 without cheating</h2>
      <ul>
        <li>Improve the report until blind reviewers cite the intended risky nodes with less prompting.</li>
        <li>Calibrate aliases and accepted decisions only when they match defensible review behavior.</li>
        <li>Keep benign cases in the suite so always-blocking cannot win.</li>
        <li>Add harder cases as the score rises, especially near-miss and low-signal drift.</li>
      </ul>
    </section>
  </main>
</body>
</html>
`;
}

export function writeBlindPacket(caseDef, fixture) {
  const packetDir = safeOutputPath("packets", caseDef.id);
  rmSync(packetDir, { recursive: true, force: true });
  mkdirSync(packetDir, { recursive: true });

  const report = runDiffDrift(fixture.repoPath, fixture.stateHome, "md");
  writeFileSync(join(packetDir, "prompt.md"), promptFor(caseDef));
  writeFileSync(join(packetDir, "diff-drift-report.md"), report.stdout);
  writeFileSync(join(packetDir, "raw-git-diff.patch"), gitDiff(fixture.repoPath));
  writeFileSync(
    join(packetDir, "metadata.json"),
    `${JSON.stringify(
      {
        caseId: caseDef.id,
        title: caseDef.title,
        project: caseDef.repo?.project ?? caseDef.id,
        branch: caseDef.repo?.branch ?? "agent/eval-drift",
        generatedAt: new Date().toISOString(),
      },
      null,
      2,
    )}\n`,
  );
  return packetDir;
}

function promptFor(caseDef) {
  return `${caseDef.blindPrompt ?? defaultPrompt(caseDef)}

Return only JSON with this shape:

\`\`\`json
{
  "caseId": "${caseDef.id}",
  "decision": "approve | investigate | block",
  "confidence": 0.0,
  "findings": [
    {
      "title": "Short risk title",
      "severity": "high | medium | low",
      "filePath": "path/from/repo/root.ts",
      "riskType": "Diff Drift flag type or your concise category",
      "evidence": "Specific code or report evidence"
    }
  ]
}
\`\`\`
`;
}

function defaultPrompt(caseDef) {
  return `You are a blind reviewer evaluating an AI agent's code drift for "${caseDef.title}".

Use the attached Diff Drift report and raw git diff. Decide whether the change should be approved, investigated, or blocked before trust. Do not assume Diff Drift is correct; cite concrete evidence.`;
}

function safeOutputPath(...parts) {
  const abs = resolve(evalOutputRoot, ...parts);
  const root = resolve(evalOutputRoot);
  if (abs !== root && !abs.startsWith(`${root}\\`) && !abs.startsWith(`${root}/`)) {
    throw new Error(`Eval output path escapes .eval: ${parts.join("/")}`);
  }
  return abs;
}

function bar(score) {
  const filled = Math.round(score / 10);
  return `[${"#".repeat(filled)}${".".repeat(10 - filled)}]`;
}

function percent(value) {
  return `${Math.round((value ?? 0) * 100)}%`;
}

function notesFor(score) {
  const notes = [];
  if (!score.decisionAccepted) {
    notes.push(`decision expected ${score.acceptedDecisions.join("/")}`);
  }
  if (score.missedExpectations?.length) {
    notes.push(`missed ${score.missedExpectations.map(expectationShortName).join(", ")}`);
  }
  if (score.falsePositives) {
    notes.push(`${score.falsePositives} unmatched`);
  }
  return notes.length ? notes.join("; ") : "clean";
}

function sortedScores(scores = []) {
  return [...scores].sort((a, b) => a.score - b.score || a.caseId.localeCompare(b.caseId));
}

function expectationShortName(expectation) {
  const parts = String(expectation).split(" / ").map((part) => part.trim()).filter(Boolean);
  return parts[1] ?? parts[0] ?? expectation;
}

function renderCaseCard(score) {
  const recall = Math.round((score.recall ?? 0) * 100);
  const localization = Math.round((score.localization ?? 0) * 100);
  const notes = notesFor(score);
  return `<article class="case">
  <div>
    <div class="case-title">${escapeHtml(score.caseId)}</div>
    <div class="case-meta">score ${score.score}/100</div>
  </div>
  <div class="barline">
    ${barLine("Score", score.score, "var(--accent)")}
    ${barLine("Finding recall", recall, "var(--green)")}
    ${barLine("Localization", localization, "var(--blue)")}
  </div>
  <div>
    <div class="chips">
      <span class="chip ${score.decisionAccepted ? "ok" : "miss"}">decision ${score.decisionAccepted ? "ok" : "miss"}</span>
      <span class="chip">${score.matchedFindings}/${score.requiredFindings} findings</span>
      <span class="chip">${score.falsePositives} unmatched</span>
    </div>
    <p class="why"><b>Why:</b> ${escapeHtml(notes)}</p>
  </div>
</article>`;
}

function renderScoreDot(score, index) {
  const x = clampScore(score.score);
  const y = 12 + (index % 4) * 12;
  return `<span class="dot" title="${escapeHtml(score.caseId)}: ${score.score}" style="--x: ${x}%; --y: ${y}px"></span>`;
}

function barLine(label, value, fill) {
  return `<div>
    <div class="bar-label"><span>${escapeHtml(label)}</span><span>${value}%</span></div>
    <div class="bar" style="--fill: ${fill}; --value: ${clampScore(value)}%"><i></i></div>
  </div>`;
}

function metric(label, value) {
  return `<div class="metric"><b>${percent(value)}</b><span>${escapeHtml(label)}</span></div>`;
}

function clampScore(value) {
  return Math.max(0, Math.min(100, Math.round(value ?? 0)));
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
