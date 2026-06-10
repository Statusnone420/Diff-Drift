#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { loadCases, projectRoot } from "./lib/cases.mjs";
import { evalOutputRoot, writeAgentScores } from "./lib/packets.mjs";
import { scoreAgentAnswer, summarizeAgentScores } from "./lib/score.mjs";

const answerFiles = collectAnswerFiles(process.argv.slice(2));
if (answerFiles.length === 0) {
  throw new Error("No agent answers found. Save JSON answers under .eval/answers or pass file paths.");
}

const cases = await loadCases();
const byId = new Map(cases.map((caseDef) => [caseDef.id, caseDef]));
const scores = [];

for (const file of answerFiles) {
  const answer = JSON.parse(readFileSync(file, "utf8"));
  const caseId = answer.caseId ?? idFromFile(file);
  const caseDef = byId.get(caseId);
  if (!caseDef) {
    throw new Error(`No eval case found for answer ${file} with caseId "${caseId}"`);
  }
  const score = scoreAgentAnswer(caseDef, { ...answer, caseId });
  scores.push({ answerFile: file, evaluator: normalizeEvaluator(answer.evaluator), ...score });
  console.log(
    `SCORE ${caseId}: ${score.score}/100 decision=${score.decisionCorrect ? "ok" : "miss"} recall=${score.recall.toFixed(
      2,
    )}`,
  );
}

const evaluators = collectEvaluators(scores);
const result = {
  generatedAt: new Date().toISOString(),
  averageScore: Math.round(scores.reduce((sum, score) => sum + score.score, 0) / scores.length),
  summary: summarizeAgentScores(scores),
  evaluators,
  // Honest by construction: a single evaluator — or any all-model panel —
  // is not independent external validation. The banner clears only when a
  // human evaluator outside the project has contributed answers.
  externalValidationPending: evaluators.length < 2 || evaluators.every((e) => e.kind !== "human"),
  scores,
};
writeAgentScores(result);
console.log(`\nAgent eval average: ${result.averageScore}/100 over ${scores.length} answer(s)`);
console.log(
  `Evaluators: ${evaluators.map((e) => `${e.id} (${e.kind}, ${e.cases} case${e.cases === 1 ? "" : "s"})`).join(", ")}`,
);
if (result.externalValidationPending) {
  console.log("Note: independent external validation pending — see the scorecard banner.");
}
console.log(`Scorecard: ${join(evalOutputRoot, "results", "agents", "latest.html")}`);

function collectAnswerFiles(args) {
  if (args.length > 0) {
    return args.map((arg) => resolve(projectRoot, arg));
  }
  const answersDir = join(evalOutputRoot, "answers");
  if (!existsSync(answersDir)) {
    return [];
  }
  return readdirSync(answersDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".json"))
    .map((entry) => join(answersDir, entry.name))
    .sort();
}

function idFromFile(file) {
  return file.split(/[\\/]/).pop().replace(/\.json$/i, "");
}

function normalizeEvaluator(evaluator) {
  if (!evaluator || typeof evaluator !== "object") {
    return { id: "unspecified", kind: "unknown" };
  }
  const kind = ["model", "human"].includes(evaluator.kind) ? evaluator.kind : "unknown";
  return {
    id: typeof evaluator.id === "string" && evaluator.id.trim() ? evaluator.id.trim() : "unspecified",
    kind,
    ...(typeof evaluator.note === "string" && evaluator.note.trim() ? { note: evaluator.note.trim() } : {}),
  };
}

function collectEvaluators(scored) {
  const byId = new Map();
  for (const score of scored) {
    const key = `${score.evaluator.id}|${score.evaluator.kind}`;
    const entry = byId.get(key) ?? { ...score.evaluator, cases: 0, scoreSum: 0 };
    entry.cases += 1;
    entry.scoreSum += score.score;
    byId.set(key, entry);
  }
  return [...byId.values()]
    .map(({ scoreSum, ...entry }) => ({ ...entry, averageScore: Math.round(scoreSum / entry.cases) }))
    .sort((a, b) => a.id.localeCompare(b.id));
}
