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
  ];
  if (result.evaluators?.length) {
    lines.push(
      `Evaluators: ${result.evaluators
        .map((e) => `${e.id} (${evaluatorKind(e)}, ${e.cases} case${e.cases === 1 ? "" : "s"})`)
        .join(", ")}`,
      "",
    );
  }
  if (result.externalValidationPending) {
    lines.push(
      "> **Independent external validation pending.** All answers so far come from a single evaluator or an all-model panel. Treat the score as an internal product-quality signal, not third-party validation.",
      "",
    );
  }
  lines.push(
    `Overall score: ${bar(result.averageScore)} ${result.averageScore}/100`,
    "",
    `- Decision accuracy: ${percent(summary.decisionAccuracy)}`,
    `- Finding recall: ${percent(summary.averageRecall)}`,
    `- Localization: ${percent(summary.averageLocalization)}`,
  );
  if (summary.precision !== undefined) {
    const matchedReported = summary.matchedReportedFindings ?? summary.matchedFindings ?? 0;
    const related = summary.totalRelatedFindings ?? 0;
    const falsePositives = summary.totalFalsePositives ?? 0;
    lines.push(
      `- Precision: ${percent(summary.precision)} (${matchedReported} matched, ${related} related, ${falsePositives} false positives across ${summary.totalFindings} reported findings)`,
    );
  }
  lines.push(
    "",
    "| Case | Score | Decision | Recall | Notes |",
    "| --- | ---: | --- | ---: | --- |",
  );

  for (const score of sortedScores(result.scores)) {
    lines.push(
      `| ${score.caseId} | ${score.score} | ${score.decisionAccepted ? "ok" : "miss"} (${score.acceptedDecisions.join("/")}) | ${percent(
        score.recall,
      )} | ${notesFor(score)} |`,
    );
  }

  const perRule = Object.entries(summary.perRuleRecall ?? {});
  if (perRule.length > 0) {
    lines.push(
      "",
      "## Per-rule recall",
      "",
      "Across every case that required the flag type:",
      "",
      "| Flag type | Matched / Required | Recall |",
      "| --- | ---: | ---: |",
    );
    for (const [type, entry] of perRule) {
      lines.push(`| ${type} | ${entry.matched}/${entry.required} | ${percent(entry.recall)} |`);
    }
  }

  lines.push(
    "",
    "## Improvement loop",
    "",
    "- Improve Diff Drift output so blind reviewers find the same risky nodes with less ambiguity.",
    "- Add harder cases and keep benign cases in the mix so the score cannot rise by always blocking.",
    "- Treat scorer changes as rubric calibration: aliases and accepted decisions should reflect defensible human review, not hide misses.",
    "- Review misses in `missedExpectations` and unmatched findings before changing the product or rubric.",
  );
  return `${lines.join("\n")}\n`;
}

export function renderAgentDashboard(result) {
  const summary = result.summary ?? {};
  const score = clampScore(result.averageScore);
  const scores = sortedScores(result.scores);
  const caseRows = scores.map(renderCaseRow).join("\n");
  const histogram = renderScoreHistogram(scores);
  const scoreRange = describeScoreRange(scores);
  const caseCount = result.scores?.length ?? 0;
  const evaluatorCount = result.evaluators?.length ?? 0;
  const answerCount =
    result.evaluators?.reduce((sum, evaluator) => sum + (evaluator.cases ?? 0), 0) ?? caseCount;
  const falsePositives = summary.totalFalsePositives ?? 0;
  const metrics = [
    metric("Decision accuracy", summary.decisionAccuracy, "var(--green)"),
    metric("Finding recall", summary.averageRecall, "var(--teal)"),
    metric("Localization", summary.averageLocalization, "var(--blue)"),
    ...(summary.precision !== undefined ? [metric("Precision", summary.precision, "var(--purple)")] : []),
  ];
  const evaluatorLine = result.evaluators?.length
    ? `<span>${escapeHtml(result.evaluators.map((e) => `${e.id} (${evaluatorKind(e)}, ${e.cases})`).join(" | "))}</span>`
    : "<span>No evaluator metadata</span>";
  const pendingBanner = result.externalValidationPending
    ? `<p class="pending"><b>Independent external validation pending.</b> All answers so far come from a model-only panel. Treat this as an internal product-quality signal, not third-party validation.</p>`
    : "";
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Diff Drift Blind-Agent Scorecard</title>
  <style>
    :root {
      color-scheme: light;
      --page: #f6f8fb;
      --surface: #ffffff;
      --surface-quiet: #f0f4f8;
      --border: #d9e1eb;
      --border-strong: #b9c6d6;
      --ink: #152032;
      --text: #2b3547;
      --dim: #566276;
      --mute: #718096;
      --accent: #d89024;
      --amber-soft: #fff4dc;
      --green: #15824f;
      --green-soft: #e7f6ee;
      --teal: #15858f;
      --teal-soft: #e4f6f8;
      --blue: #3468c7;
      --blue-soft: #e8effd;
      --purple: #7650bd;
      --purple-soft: #f0eafa;
      --red: #c24135;
      --red-soft: #fff0ee;
      --mono: "Cascadia Code", "JetBrains Mono", ui-monospace, Consolas, monospace;
      --ui: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--page);
      color: var(--text);
      font-family: var(--ui);
      font-size: 13px;
      line-height: 1.45;
    }
    main { max-width: 1800px; min-width: 1180px; margin: 0 auto; padding: 18px 28px 20px; }
    .topline {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 18px;
      margin-bottom: 10px;
      color: var(--dim);
      font-size: 12px;
    }
    .brand { display: flex; align-items: center; gap: 10px; color: var(--ink); font-weight: 700; }
    .mark {
      width: 24px;
      height: 24px;
      border-radius: 6px;
      background: linear-gradient(145deg, #2a2d37, #15171d);
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
    .run-meta {
      max-width: 720px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      text-align: center;
    }
    .hero {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 14px 18px;
      display: grid;
      grid-template-columns: minmax(0, 1fr) 560px;
      gap: 28px;
      align-items: stretch;
      margin-bottom: 10px;
    }
    .hero-copy {
      display: flex;
      flex-direction: column;
      justify-content: space-between;
    }
    .label-row {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-bottom: 10px;
    }
    .label {
      border-radius: 999px;
      padding: 4px 9px;
      background: var(--surface-quiet);
      color: var(--dim);
      border: 1px solid var(--border);
      font-size: 12px;
      font-weight: 650;
    }
    .label.good {
      background: var(--green-soft);
      color: var(--green);
      border-color: #b8e5cc;
    }
    h1 {
      margin: 0 0 8px;
      color: var(--ink);
      font-size: 44px;
      line-height: 1.02;
      letter-spacing: 0;
      text-wrap: balance;
    }
    .lede { max-width: 72ch; margin: 0; color: var(--dim); font-size: 15px; }
    .pending {
      margin: 12px 0 0;
      padding: 8px 10px;
      border: 1px solid #e7bf79;
      border-radius: 8px;
      background: var(--amber-soft);
      color: #6f4a10;
      font-size: 13px;
      max-width: 78ch;
    }
    .score-panel {
      border: 1px solid var(--border);
      border-radius: 8px;
      background: #fbfcfe;
      padding: 12px;
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .score-head {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 16px;
      padding-bottom: 8px;
      border-bottom: 1px solid var(--border);
    }
    .score-head h2 {
      margin: 0 0 2px;
      color: var(--ink);
      font-size: 16px;
      letter-spacing: 0;
    }
    .score-head p {
      margin: 0;
      color: var(--dim);
      font-size: 12px;
    }
    .score-value {
      text-align: right;
      color: var(--dim);
      font-family: var(--mono);
      font-size: 12px;
      white-space: nowrap;
    }
    .score-value strong {
      display: block;
      color: var(--ink);
      font-family: var(--ui);
      font-size: 34px;
      line-height: 1;
      letter-spacing: 0;
    }
    .score-bar {
      height: 8px;
      border-radius: 999px;
      background: #dfe6ef;
      overflow: hidden;
    }
    .score-bar i {
      display: block;
      height: 100%;
      width: ${score}%;
      background: var(--accent);
    }
    .metric-list {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 7px 12px;
    }
    .metric-row {
      display: grid;
      grid-template-columns: 104px minmax(0, 1fr) 44px;
      gap: 8px;
      align-items: center;
      color: var(--dim);
      font-size: 12px;
    }
    .metric-row strong {
      color: var(--ink);
      font-family: var(--mono);
      font-size: 12px;
      text-align: right;
    }
    .metric-track,
    .hist-track {
      height: 7px;
      border-radius: 999px;
      background: #dfe6ef;
      overflow: hidden;
    }
    .metric-track i,
    .hist-track i {
      display: block;
      height: 100%;
      width: var(--value);
      background: var(--fill);
    }
    .panel-title {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 12px;
      margin-bottom: 8px;
      color: var(--ink);
      font-weight: 700;
    }
    .hint { color: var(--mute); font-size: 12px; font-weight: 500; }
    .distribution {
      border-top: 1px solid var(--border);
      padding-top: 8px;
    }
    .histogram {
      display: grid;
      gap: 5px;
    }
    .hist-row {
      display: grid;
      grid-template-columns: 42px minmax(0, 1fr) 34px;
      gap: 8px;
      align-items: center;
      color: var(--mute);
      font: 11px var(--mono);
    }
    .hist-row.active {
      color: var(--ink);
      font-weight: 700;
    }
    .hist-row.active .hist-track {
      background: var(--amber-soft);
    }
    .cases-panel {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 8px;
      overflow: hidden;
    }
    .cases-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 18px;
      padding: 8px 16px;
      border-bottom: 1px solid var(--border);
      background: #fbfcfe;
    }
    h2 { margin: 0; color: var(--ink); font-size: 17px; letter-spacing: 0; }
    .case-table {
      width: 100%;
      border-collapse: collapse;
      table-layout: fixed;
    }
    .case-table th {
      text-align: left;
      color: var(--mute);
      font-size: 11px;
      font-weight: 700;
      padding: 5px 14px;
      border-bottom: 1px solid var(--border);
      background: #f8fafc;
    }
    .case-table td {
      padding: 4px 14px;
      border-bottom: 1px solid #edf1f6;
      vertical-align: middle;
    }
    .case-table tr:last-child td { border-bottom: 0; }
    .case-name {
      color: var(--ink);
      font-weight: 750;
      overflow-wrap: anywhere;
    }
    .case-sub {
      color: var(--mute);
      font-family: var(--mono);
      font-size: 11px;
      margin-top: 1px;
      display: none;
    }
    .score-cell {
      display: grid;
      grid-template-columns: 42px minmax(0, 1fr);
      gap: 10px;
      align-items: center;
    }
    .score-pill {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      height: 26px;
      border-radius: 999px;
      background: var(--amber-soft);
      color: #6f4a10;
      font-family: var(--mono);
      font-weight: 700;
      font-size: 12px;
    }
    .bar {
      height: 7px;
      border-radius: 999px;
      background: #dfe6ef;
      overflow: hidden;
    }
    .bar > i {
      display: block;
      height: 100%;
      width: var(--value);
      background: var(--fill);
    }
    .mini-metrics {
      display: flex;
      flex-wrap: wrap;
      gap: 5px;
    }
    .mini-row {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      border-radius: 999px;
      padding: 2px 7px;
      background: #f8fafc;
      border: 1px solid var(--border);
      color: var(--dim);
      font-size: 11px;
      white-space: nowrap;
    }
    .mini-row b { color: var(--ink); font-weight: 650; }
    .chip {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      min-height: 18px;
      border: 1px solid var(--border);
      border-radius: 999px;
      padding: 1px 7px;
      font-size: 10.5px;
      font-weight: 650;
      white-space: nowrap;
    }
    .chip.ok { color: var(--green); border-color: #b8e5cc; background: var(--green-soft); }
    .chip.miss { color: var(--red); border-color: #f0bbb6; background: var(--red-soft); }
    .finding-stack {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
    }
    .finding-stack .chip { color: var(--dim); background: #f9fbfd; }
    .why {
      color: var(--dim);
      font-size: 12px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    @media (max-width: 1180px) {
      main { min-width: 0; }
      .hero { grid-template-columns: 1fr; }
      .metric-list { grid-template-columns: 1fr; }
      .case-table { min-width: 980px; }
      .cases-panel { overflow-x: auto; }
    }
  </style>
</head>
<body>
  <main>
    <div class="topline">
      <div class="brand"><span class="mark"></span><span>Diff Drift benchmark</span></div>
      <div class="run-meta">${evaluatorLine}</div>
      <div>${escapeHtml(result.generatedAt)}</div>
    </div>
    <section class="hero">
      <div class="hero-copy">
        <div>
          <div class="label-row">
            <span class="label good">${caseCount} cases</span>
            <span class="label">${evaluatorCount} model batches</span>
            <span class="label">${answerCount} blind answers</span>
            <span class="label">${falsePositives} false positives</span>
          </div>
          <h1>Blind-agent scorecard</h1>
          <p class="lede">Can a reviewer use Diff Drift packets to make the right trust decision and cite the right evidence? This report scores that reviewer workflow. It is advisory evidence for product quality, not a release gate.</p>
        </div>
        ${pendingBanner}
      </div>
      <aside class="score-panel" aria-label="Benchmark summary">
        <div class="score-head">
          <div>
            <h2>Benchmark summary</h2>
            <p>Packet-only blind review, model-only panel</p>
          </div>
          <div class="score-value"><strong>${score}</strong>/100</div>
        </div>
        <div class="score-bar" aria-hidden="true"><i></i></div>
        <div class="metric-list">
          ${metrics.join("\n          ")}
        </div>
        <section class="distribution" aria-label="Case score distribution">
          <div class="panel-title">
            <span>Case score histogram</span>
            <span class="hint">${escapeHtml(scoreRange)}</span>
          </div>
          <div class="histogram">${histogram}</div>
        </section>
      </aside>
    </section>
    <section class="cases-panel" aria-label="Case results">
      <div class="cases-head">
        <h2>Case results</h2>
        <div class="hint">Weighted recall, decision accuracy, top-risk ranking, localization, and unmatched findings.</div>
      </div>
      <table class="case-table">
        <thead>
          <tr>
            <th style="width: 23%">Case</th>
            <th style="width: 18%">Score</th>
            <th style="width: 11%">Decision</th>
            <th style="width: 22%">Coverage</th>
            <th style="width: 13%">Findings</th>
            <th>Notes</th>
          </tr>
        </thead>
        <tbody>
          ${caseRows}
        </tbody>
      </table>
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

What counts as a finding (benchmark v3 contract):

- \`findings\` is for concrete, actionable trust risks in the changed code only — things that would make a reviewer block or investigate.
- Benign observations, formatting remarks, mitigating context, and feedback about Diff Drift's report itself belong in \`notes\`, never in \`findings\`.
- If your decision is \`approve\`, \`findings\` should normally be an empty array.
- A file the report marks "Skipped — file too large to analyze" is not by itself a finding when its raw diff shows no concrete risk; weigh it in your decision and mention it in \`notes\`.
- Severity is scored. For risks that correspond directly to Diff Drift flags, use the report severity; lower severity misses the finding. Conservative escalation is accepted only when your evidence supports a higher severity.

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
  ],
  "notes": ["Optional benign observations or report feedback — not risks."]
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
  if (score.mislocalizedExpectations?.length) {
    notes.push(`wrong location for ${score.mislocalizedExpectations.map(expectationShortName).join(", ")}`);
  }
  if (score.falsePositives) {
    notes.push(`${score.falsePositives} unmatched`);
  }
  if (score.benignWrongDecision) {
    notes.push("wrong decision on benign case");
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

function renderCaseRow(score) {
  const recall = Math.round((score.recall ?? 0) * 100);
  const localization = Math.round((score.localization ?? 0) * 100);
  const notes = notesFor(score);
  const related = score.relatedFindings?.length ?? 0;
  const unmatched = score.falsePositives ?? 0;
  const findingChips = [
    `<span class="chip">${score.matchedFindings}/${score.requiredFindings}</span>`,
    `<span class="chip">${unmatched} unmatched</span>`,
    ...(related ? [`<span class="chip">${related} related</span>`] : []),
  ].join("\n      ");
  return `<tr>
  <td>
    <div class="case-name">${escapeHtml(score.caseId)}</div>
    <div class="case-sub">score ${score.score}/100</div>
  </td>
  <td>
    <div class="score-cell">
      <span class="score-pill">${score.score}</span>
      ${barLine(score.score, "var(--accent)")}
    </div>
  </td>
  <td><span class="chip ${score.decisionAccepted ? "ok" : "miss"}">${score.decisionAccepted ? "ok" : "miss"}</span></td>
  <td>
    <div class="mini-metrics">
      ${miniMetric("Recall", recall)}
      ${miniMetric("Locate", localization)}
    </div>
  </td>
  <td>
    <div class="finding-stack">
      ${findingChips}
    </div>
  </td>
  <td><div class="why">${escapeHtml(notes)}</div></td>
</tr>`;
}

function barLine(value, fill) {
  return `<div class="bar" style="--fill: ${fill}; --value: ${clampScore(value)}%"><i></i></div>`;
}

function miniMetric(label, value) {
  return `<div class="mini-row">
    <b>${escapeHtml(label)}</b>
    <span>${value}%</span>
  </div>`;
}

function metric(label, value, fill) {
  const amount = Math.round((value ?? 0) * 100);
  return `<div class="metric-row">
    <span>${escapeHtml(label)}</span>
    <div class="metric-track" style="--fill: ${fill}; --value: ${clampScore(amount)}%"><i></i></div>
    <strong>${amount}%</strong>
  </div>`;
}

function renderScoreHistogram(scores) {
  const buckets = [
    { label: "0-59", min: 0, max: 59 },
    { label: "60-79", min: 60, max: 79 },
    { label: "80-89", min: 80, max: 89 },
    { label: "90-99", min: 90, max: 99 },
    { label: "100", min: 100, max: 100 },
  ];
  const counts = buckets.map(
    (bucket) =>
      scores.filter((score) => {
        const value = clampScore(score.score);
        return value >= bucket.min && value <= bucket.max;
      }).length,
  );
  const maxCount = Math.max(1, ...counts);
  return buckets
    .map((bucket, index) => {
      const count = counts[index];
      return `<div class="hist-row ${count > 0 ? "active" : ""}">
    <span>${escapeHtml(bucket.label)}</span>
    <div class="hist-track" style="--fill: var(--accent); --value: ${Math.round((count / maxCount) * 100)}%"><i></i></div>
    <strong>${count}</strong>
  </div>`;
    })
    .join("\n");
}

function evaluatorKind(evaluator) {
  return evaluator.external === true ? `external ${evaluator.kind}` : evaluator.kind;
}

function describeScoreRange(scores) {
  if (scores.length === 0) {
    return "No cases";
  }
  const values = scores.map((score) => clampScore(score.score)).sort((a, b) => a - b);
  const min = values[0];
  const max = values[values.length - 1];
  const median = values[Math.floor(values.length / 2)];
  return `range ${min}-${max}; median ${median}`;
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
