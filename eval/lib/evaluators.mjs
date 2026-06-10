export function normalizeEvaluator(evaluator) {
  if (!evaluator || typeof evaluator !== "object") {
    return { id: "unspecified", kind: "unknown" };
  }
  const kind = ["model", "human"].includes(evaluator.kind) ? evaluator.kind : "unknown";
  const normalized = {
    id: typeof evaluator.id === "string" && evaluator.id.trim() ? evaluator.id.trim() : "unspecified",
    kind,
  };
  if (kind === "human") {
    normalized.external = evaluator.external === true;
  }
  if (typeof evaluator.note === "string" && evaluator.note.trim()) {
    normalized.note = evaluator.note.trim();
  }
  return normalized;
}

export function collectEvaluators(scored) {
  const byId = new Map();
  for (const score of scored) {
    const evaluator = normalizeEvaluator(score.evaluator);
    const key = `${evaluator.id}|${evaluator.kind}|${evaluator.external === true ? "external" : "internal"}`;
    const entry = byId.get(key) ?? { ...evaluator, cases: 0, scoreSum: 0 };
    entry.cases += 1;
    entry.scoreSum += score.score;
    byId.set(key, entry);
  }
  return [...byId.values()]
    .map(({ scoreSum, ...entry }) => ({ ...entry, averageScore: Math.round(scoreSum / entry.cases) }))
    .sort((a, b) => a.id.localeCompare(b.id));
}

// Per-case external-review coverage. A case counts as externally validated
// only if one of its answers came from a human evaluator explicitly marked
// `external: true`. Keyed by caseId so partial coverage (one external human on
// a subset of cases) is visible, not hidden by the aggregated evaluator list.
export function summarizeExternalValidation(scored) {
  const caseHasExternal = new Map();
  const evaluatorKeys = new Set();
  scored.forEach((score, index) => {
    const evaluator = normalizeEvaluator(score.evaluator);
    evaluatorKeys.add(`${evaluator.id}|${evaluator.kind}|${evaluator.external === true ? "ext" : "int"}`);
    const caseKey = score.caseId ?? score.answerFile ?? `#${index}`;
    const isExternalHuman = evaluator.kind === "human" && evaluator.external === true;
    caseHasExternal.set(caseKey, (caseHasExternal.get(caseKey) ?? false) || isExternalHuman);
  });
  return {
    evaluatorCount: evaluatorKeys.size,
    externalCases: [...caseHasExternal.values()].filter(Boolean).length,
    totalCases: caseHasExternal.size,
  };
}

export function externalValidationPending(coverage) {
  // Independent validation requires at least two evaluators AND an external
  // human review on EVERY case. Zero or partial external coverage stays pending.
  return (
    coverage.evaluatorCount < 2 ||
    coverage.totalCases === 0 ||
    coverage.externalCases < coverage.totalCases
  );
}
