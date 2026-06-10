#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { collectAnswerFiles } from "./lib/answers.mjs";
import { loadCases, projectRoot } from "./lib/cases.mjs";
import {
  collectEvaluators,
  externalValidationPending,
  normalizeEvaluator,
  summarizeExternalValidation,
} from "./lib/evaluators.mjs";
import { evalOutputRoot, writeAgentScores } from "./lib/packets.mjs";
import { scoreAgentAnswer, summarizeAgentScores } from "./lib/score.mjs";

const answerFiles = collectAnswerFiles(
  process.argv.slice(2),
  join(evalOutputRoot, "answers"),
  projectRoot,
);
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

// Each case should be answered once per evaluator; a repeated (caseId,
// evaluator) pair would let duplicate answers skew the averages.
const seenCaseEvaluator = new Set();
for (const score of scores) {
  const key = `${score.caseId}|${score.evaluator?.id ?? "unspecified"}`;
  if (seenCaseEvaluator.has(key)) {
    console.warn(
      `WARNING: case "${score.caseId}" answered more than once by "${score.evaluator?.id ?? "unspecified"}" — every answer is scored and averaged.`,
    );
  }
  seenCaseEvaluator.add(key);
}

const evaluators = collectEvaluators(scores);
const externalValidation = summarizeExternalValidation(scores);
const result = {
  generatedAt: new Date().toISOString(),
  averageScore: Math.round(scores.reduce((sum, score) => sum + score.score, 0) / scores.length),
  summary: summarizeAgentScores(scores),
  evaluators,
  externalValidation,
  // Honest by construction: a single evaluator, any all-model panel, an
  // internal human pass, or partial external coverage is not independent
  // external validation.
  externalValidationPending: externalValidationPending(externalValidation),
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

function idFromFile(file) {
  return file.split(/[\\/]/).pop().replace(/\.json$/i, "");
}
