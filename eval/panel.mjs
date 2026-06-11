#!/usr/bin/env node
// Multi-model evaluator panel for the blind-agent benchmark.
//
//   npm run eval:panel
//
// Different models each play the blind reviewer over the SAME v4 packets. The
// useful output is their agreement, not a ranking: the models are rulers, Diff
// Drift is the object being measured. This script reuses the frozen scorer
// (eval/lib/score.mjs — unchanged) per model and reports each model's score, the
// spread across models, and a model x case matrix that separates a real
// product-clarity gap (every model misses a case) from ruler noise (one does).
//
// Columns are auto-discovered, so adding a model is just dropping a folder:
//   - opus-4-8        -> eval/benchmarks/v4/answers           (the canonical Opus column)
//   - <model>         -> eval/benchmarks/v4/panel/<model>/     (sonnet-4-6, haiku-4-5, gpt-5-5, gemini, ...)
//
// Per-model + spread is reported deliberately instead of one pooled mean:
// pooling a weak ruler with a frontier one would move the headline for the
// wrong reason. See docs/wiki/Eval-Methodology.md (Multi-model panel).

import { existsSync, readdirSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { collectAnswerFiles } from "./lib/answers.mjs";
import { loadCases, projectRoot } from "./lib/cases.mjs";
import { scoreAgentAnswer } from "./lib/score.mjs";
import { renderPanelHtml } from "./lib/panel-render.mjs";
import { readFileSync } from "node:fs";

const V4 = join(projectRoot, "eval", "benchmarks", "v4");
const PANEL_DIR = join(V4, "panel");

// Display labels + ordering hints for known models; unknown dirs fall back to
// the folder name so a cross-vendor drop-in still renders.
const MODEL_META = {
  "opus-4-8": { label: "Claude Opus 4.8", vendor: "Anthropic" },
  "sonnet-4-6": { label: "Claude Sonnet 4.6", vendor: "Anthropic" },
  "haiku-4-5": { label: "Claude Haiku 4.5", vendor: "Anthropic" },
  "fable-5": { label: "Fable 5", vendor: "Anthropic" },
  "gpt-5-5": { label: "GPT-5.5", vendor: "OpenAI" },
  "gemini": { label: "Gemini", vendor: "Google" },
};

function metaFor(key) {
  return MODEL_META[key] ?? { label: key, vendor: "—" };
}

function hasAnswers(dir) {
  return existsSync(dir) && readdirSync(dir).some((f) => f.endsWith(".json"));
}

// Build the column list: Opus from the canonical answers dir, every panel/<model>/
// with answers as its own column.
function discoverModels() {
  const models = [];
  const opusDir = join(V4, "answers");
  if (hasAnswers(opusDir)) models.push({ key: "opus-4-8", dir: opusDir });
  if (existsSync(PANEL_DIR)) {
    for (const entry of readdirSync(PANEL_DIR, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const dir = join(PANEL_DIR, entry.name);
      if (hasAnswers(dir)) models.push({ key: entry.name, dir });
    }
  }
  return models;
}

const cases = await loadCases();
const byId = new Map(cases.map((c) => [c.id, c]));
const models = discoverModels();
if (models.length === 0) {
  throw new Error(`No model answer folders found. Expected ${join(V4, "answers")} and/or ${PANEL_DIR}/<model>/.`);
}

// Score every model over every case it answered.
const scored = models.map(({ key, dir }) => {
  const files = collectAnswerFiles([dir], dir, projectRoot);
  const perCase = {};
  let decisionHits = 0;
  let recallSum = 0;
  for (const file of files) {
    const answer = JSON.parse(readFileSync(file, "utf8"));
    const caseId = answer.caseId;
    const caseDef = byId.get(caseId);
    if (!caseDef) throw new Error(`Panel: no eval case for ${file} (caseId "${caseId}")`);
    const s = scoreAgentAnswer(caseDef, { ...answer, caseId });
    perCase[caseId] = { score: s.score, decisionAccepted: s.decisionAccepted, recall: s.recall };
    if (s.decisionAccepted) decisionHits += 1;
    recallSum += s.recall;
  }
  const count = files.length;
  const total = count ? Math.round(Object.values(perCase).reduce((a, b) => a + b.score, 0) / count) : 0;
  const meta = metaFor(key);
  return {
    key,
    label: meta.label,
    vendor: meta.vendor,
    cases: count,
    total,
    decisionAccuracy: count ? decisionHits / count : 0,
    recall: count ? recallSum / count : 0,
    perCase,
  };
});

// Case order: hardest (lowest mean across models) first, so the matrix leads
// with the cases that carry signal.
const caseIds = cases.map((c) => c.id);
function caseMean(caseId) {
  const xs = scored.map((m) => m.perCase[caseId]?.score).filter((x) => x != null);
  return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : 100;
}
const orderedCases = [...caseIds].sort((a, b) => caseMean(a) - caseMean(b));

// Per-case agreement: clean (every model 100), product-signal (every model
// misses), or split (models disagree).
const matrix = orderedCases.map((caseId) => {
  const row = scored.map((m) => m.perCase[caseId]?.score ?? null).filter((x) => x != null);
  const all100 = row.length > 0 && row.every((s) => s === 100);
  const allMiss = row.length > 0 && row.every((s) => s < 100);
  const agreement = all100 ? "clean" : allMiss ? "product-signal" : "split";
  const caseDef = byId.get(caseId);
  return {
    caseId,
    title: caseDef?.title ?? caseId,
    scores: Object.fromEntries(scored.map((m) => [m.key, m.perCase[caseId]?.score ?? null])),
    mean: Math.round(caseMean(caseId)),
    agreement,
  };
});

const totals = scored.map((m) => m.total);
const spread = { min: Math.min(...totals), max: Math.max(...totals) };

const result = {
  benchmark: "v4",
  models: scored.sort((a, b) => b.total - a.total || a.label.localeCompare(b.label)),
  cases: orderedCases.map((id) => ({ caseId: id, title: byId.get(id)?.title ?? id })),
  matrix,
  spread,
  productSignals: matrix.filter((r) => r.agreement === "product-signal").map((r) => r.caseId),
  splits: matrix.filter((r) => r.agreement === "split").map((r) => r.caseId),
  // The panel stays model-only: independent external validation needs a human
  // outside the project, which no folder here provides.
  externalValidationPending: true,
};

// Console summary.
console.log(`\nBlind-agent multi-model panel (benchmark v4) — ${result.models.length} models, ${orderedCases.length} cases\n`);
for (const m of result.models) {
  console.log(
    `  ${m.label.padEnd(20)} ${String(m.total).padStart(3)}/100   decision ${Math.round(m.decisionAccuracy * 100)}%   recall ${Math.round(m.recall * 100)}%`,
  );
}
console.log(`\n  Spread: ${spread.min}–${spread.max} across ${result.models.length} models`);
if (result.productSignals.length) {
  console.log(`  Product-clarity signals (every model misses): ${result.productSignals.join(", ")}`);
}
if (result.splits.length) {
  console.log(`  Splits (models disagree — ruler noise): ${result.splits.join(", ")}`);
}

// Write artifacts. Stamp the time here (scripts may use Date directly).
const stamped = { generatedAt: new Date().toISOString(), ...result };
mkdirSync(PANEL_DIR, { recursive: true });
writeFileSync(join(PANEL_DIR, "panel-scorecard.json"), JSON.stringify(stamped, null, 2));
writeFileSync(join(PANEL_DIR, "panel-scorecard.md"), renderPanelMd(stamped));
writeFileSync(join(PANEL_DIR, "panel-scorecard.html"), renderPanelHtml(stamped));
console.log(`\n  Wrote eval/benchmarks/v4/panel/panel-scorecard.{json,md,html}`);
console.log(`  Capture the image with: npm run scorecard:capture -- eval/benchmarks/v4/panel/panel-scorecard.html docs/assets/diff-drift-blind-agent-scorecard.png\n`);

function renderPanelMd(r) {
  const lines = [];
  lines.push(`# Diff Drift — blind-agent multi-model panel (benchmark v4)`);
  lines.push("");
  lines.push(`Generated: ${r.generatedAt}`);
  lines.push("");
  lines.push(
    `> The models are rulers; Diff Drift is the object being measured. Read the per-model scores and the spread — not a pooled average. An all-model panel stays **independent external validation pending** (that needs a human outside the project).`,
  );
  lines.push("");
  lines.push(`**Spread: ${r.spread.min}–${r.spread.max} / 100** across ${r.models.length} models.`);
  lines.push("");
  lines.push(`| Model | Vendor | Overall | Decision acc | Recall | Cases |`);
  lines.push(`| --- | --- | ---: | ---: | ---: | ---: |`);
  for (const m of r.models) {
    lines.push(
      `| ${m.label} | ${m.vendor} | ${m.total}/100 | ${Math.round(m.decisionAccuracy * 100)}% | ${Math.round(
        m.recall * 100,
      )}% | ${m.cases} |`,
    );
  }
  lines.push("");
  lines.push(`## Model × case matrix`);
  lines.push("");
  lines.push(`| Case | ${r.models.map((m) => m.label).join(" | ")} | Agreement |`);
  lines.push(`| --- | ${r.models.map(() => "---:").join(" | ")} | --- |`);
  for (const row of r.matrix) {
    const cells = r.models.map((m) => (row.scores[m.key] == null ? "·" : String(row.scores[m.key])));
    const tag = row.agreement === "clean" ? "✓ clean" : row.agreement === "product-signal" ? "⚠ all miss" : "split";
    lines.push(`| ${row.caseId} | ${cells.join(" | ")} | ${tag} |`);
  }
  lines.push("");
  if (r.productSignals.length) {
    lines.push(
      `**Product-clarity signals** (every model loses points — worth a real engine/report fix): ${r.productSignals.join(", ")}.`,
    );
  } else {
    lines.push(`**Product-clarity signals:** none — no case is missed by every model.`);
  }
  lines.push("");
  if (r.splits.length) {
    lines.push(`**Splits** (models disagree — treat as ruler noise, not a tool defect): ${r.splits.join(", ")}.`);
    lines.push("");
  }
  return lines.join("\n");
}
