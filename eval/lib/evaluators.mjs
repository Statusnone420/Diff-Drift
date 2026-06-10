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

export function externalValidationPending(evaluators) {
  return (
    evaluators.length < 2 ||
    !evaluators.some((evaluator) => evaluator.kind === "human" && evaluator.external === true)
  );
}
