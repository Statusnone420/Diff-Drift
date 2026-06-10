#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { loadCases, projectRoot } from "./lib/cases.mjs";
import { evalOutputRoot, writeAgentScores } from "./lib/packets.mjs";
import { scoreAgentAnswer } from "./lib/score.mjs";

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
  scores.push({ answerFile: file, ...score });
  console.log(
    `SCORE ${caseId}: ${score.score}/100 decision=${score.decisionCorrect ? "ok" : "miss"} recall=${score.recall.toFixed(
      2,
    )}`,
  );
}

const result = {
  generatedAt: new Date().toISOString(),
  averageScore: Math.round(scores.reduce((sum, score) => sum + score.score, 0) / scores.length),
  scores,
};
writeAgentScores(result);
console.log(`\nAgent eval average: ${result.averageScore}/100 over ${scores.length} answer(s)`);

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
