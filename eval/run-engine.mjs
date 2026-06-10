#!/usr/bin/env node
import { parseCaseArgs, loadCases } from "./lib/cases.mjs";
import { runDiffDrift } from "./lib/cli.mjs";
import { writeEngineResult } from "./lib/packets.mjs";
import { createCaseRepo } from "./lib/repo.mjs";
import { scoreEngineResult, summarizeEngineScores } from "./lib/score.mjs";

const args = parseCaseArgs(process.argv.slice(2));
const cases = await loadCases(args.ids);
const scores = [];

for (const caseDef of cases) {
  const fixture = createCaseRepo(caseDef);
  try {
    const run = runDiffDrift(fixture.repoPath, fixture.stateHome, "json");
    const score = scoreEngineResult(caseDef, run);
    scores.push(score);
    printCaseScore(score);
  } finally {
    if (!args.keep) {
      fixture.cleanup();
    } else {
      console.log(`  kept fixture: ${fixture.repoPath}`);
    }
  }
}

const summary = summarizeEngineScores(scores);
const result = {
  generatedAt: new Date().toISOString(),
  summary,
  scores,
};
writeEngineResult(result);

if (args.json) {
  console.log(JSON.stringify(result, null, 2));
}

console.log(
  `\nEngine eval: ${summary.passedCount}/${summary.total} passed${
    summary.failed ? `, ${summary.failed} failed` : ""
  }`,
);
process.exit(summary.passed ? 0 : 1);

function printCaseScore(score) {
  console.log(`${score.passed ? "PASS" : "FAIL"} ${score.caseId} - ${score.title}`);
  for (const failure of score.failures) {
    console.log(`  - ${failure}`);
  }
}
