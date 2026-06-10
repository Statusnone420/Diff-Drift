const severityScore = new Map([
  ["low", 1],
  ["medium", 2],
  ["high", 3],
]);

const flagAliases = new Map([
  // "secret" alone was too broad — it appears in evidence for unrelated risks
  // and let a non-secret finding match this type. The specific phrases below
  // still recognize a hardcoded-secret description.
  ["Hardcoded secret", ["hardcoded secret", "credential", "access key", "aws access key", "api key"]],
  ["Dependency not in lockfile", ["dependency not in lockfile", "lockfile", "dependency drift"]],
  ["npm script changed", ["npm script", "install script", "postinstall"]],
  ["Weakened cookie flags", ["weakened cookie flags", "cookie flags", "httponly", "secure", "samesite"]],
  ["Permissive logging config", ["permissive logging", "logger redaction", "redaction removed"]],
  ["Undeclared import", ["undeclared import", "undeclared dependency", "not declared"]],
]);

export function scoreEngineResult(caseDef, run) {
  const failures = [];
  const data = run.data;
  const oracle = caseDef.oracle;
  const activeFlags = (data.flags ?? []).filter((flag) => !flag.dismissed);

  expectEqual(failures, "exit code", run.exitCode, oracle.expectedExitCode);
  expectEqual(failures, "changed file count", data.session?.changedFiles, oracle.changedFiles);
  expectEqual(failures, "risk count", data.session?.riskCount, oracle.riskCount);
  if (oracle.fileCount !== undefined) {
    expectEqual(failures, "analyzed file count", data.session?.fileCount, oracle.fileCount);
  }

  const used = new Set();
  const matchedRequired = [];
  for (const expected of oracle.requiredFlags ?? []) {
    const index = activeFlags.findIndex((flag, idx) => !used.has(idx) && flagMatches(flag, expected));
    if (index === -1) {
      failures.push(`missing flag: ${describeFlagExpectation(expected)}`);
    } else {
      used.add(index);
      matchedRequired.push(activeFlags[index]);
    }
  }

  for (const forbidden of oracle.forbiddenFlags ?? []) {
    const found = activeFlags.find((flag) => flagMatches(flag, forbidden));
    if (found) {
      failures.push(`forbidden flag present: ${found.type} in ${found.filePath}`);
    }
  }

  for (const expectedFile of oracle.files ?? []) {
    const file = (data.files ?? []).find(
      (candidate) => `${candidate.dir ?? ""}${candidate.name ?? ""}` === expectedFile.path,
    );
    if (!file) {
      failures.push(`missing analyzed file: ${expectedFile.path}`);
      continue;
    }
    if (expectedFile.summary !== undefined && file.summary !== expectedFile.summary) {
      failures.push(
        `file summary mismatch for ${expectedFile.path}: expected "${expectedFile.summary}", got "${file.summary}"`,
      );
    }
    if (expectedFile.risks !== undefined && file.risks !== expectedFile.risks) {
      failures.push(
        `file risk mismatch for ${expectedFile.path}: expected ${expectedFile.risks}, got ${file.risks}`,
      );
    }
  }

  return {
    caseId: caseDef.id,
    title: caseDef.title,
    passed: failures.length === 0,
    failures,
    stats: {
      exitCode: run.exitCode,
      changedFiles: data.session?.changedFiles,
      riskCount: data.session?.riskCount,
      activeFlags: activeFlags.length,
      requiredFlags: oracle.requiredFlags?.length ?? 0,
    },
    matchedRequired,
  };
}

export function summarizeEngineScores(scores) {
  const failed = scores.filter((score) => !score.passed);
  return {
    passed: failed.length === 0,
    total: scores.length,
    failed: failed.length,
    passedCount: scores.length - failed.length,
  };
}

export function scoreAgentAnswer(caseDef, answer) {
  validateAgentAnswer(answer);
  const required = caseDef.oracle.requiredFlags ?? [];
  const findings = answer.findings;
  const matched = new Set();
  // Global: a reported finding can be credited to AT MOST ONE expected risk.
  // Once consumed it is unavailable to every other expectation, so one finding
  // cannot satisfy multiple risks (no cross-type or duplicate reuse).
  const usedFindings = new Set();
  const matchedExpectations = [];
  const missedExpectations = [];
  const mislocalizedExpectations = [];
  let weightedHit = 0;
  let weightedTotal = 0;
  let localized = 0;

  required.forEach((expected, index) => {
    const weight = severityWeight(expected.severity);
    weightedTotal += weight;
    const findingIndex = bestFindingIndex(findings, usedFindings, expected);
    const finding = findingIndex === -1 ? null : findings[findingIndex];
    if (finding) {
      matched.add(index);
      usedFindings.add(findingIndex);
      weightedHit += weight;
      if (findingFileMatches(finding, expected)) {
        localized += 1;
      } else {
        mislocalizedExpectations.push(describeFlagExpectation(expected));
      }
      matchedExpectations.push(describeFlagExpectation(expected));
    } else {
      missedExpectations.push(describeFlagExpectation(expected));
    }
  });

  const unmatchedFindings = findings.filter((_finding, index) => !usedFindings.has(index));
  const relatedFindings = [];
  const falsePositiveFindings = [];
  for (const finding of unmatchedFindings) {
    if (findingDuplicatesMatchedExpectation(finding, required, matched)) {
      falsePositiveFindings.push(finding);
    } else if (required.some((expected) => findingRelatedToExpected(finding, expected))) {
      relatedFindings.push(finding);
    } else {
      falsePositiveFindings.push(finding);
    }
  }
  const falsePositives = falsePositiveFindings.length;
  const expectedDecision = caseDef.agent?.expectedDecision ?? inferDecision(required);
  const acceptedDecisions = acceptedDecisionSet(caseDef, expectedDecision);
  const decisionAccepted = acceptedDecisions.includes(normalize(answer.decision));
  const recall = weightedTotal === 0 ? (findings.length === 0 ? 1 : 0) : weightedHit / weightedTotal;
  const localization = matched.size === 0 ? (required.length === 0 ? 1 : 0) : localized / matched.size;
  const topRisk = topRiskRankedFirst(findings, required);
  const benignWrongDecision = required.length === 0 && !decisionAccepted;
  const rawScore =
    recall * 60 +
    (decisionAccepted ? 20 : 0) +
    (topRisk ? 10 : 0) +
    localization * 10 -
    falsePositives * 5 -
    (benignWrongDecision ? 50 : 0);

  return {
    caseId: caseDef.id,
    expectedDecision,
    acceptedDecisions,
    decisionAccepted,
    decisionCorrect: decisionAccepted,
    score: Math.max(0, Math.round(rawScore)),
    recall,
    localization,
    falsePositives,
    topRisk,
    benignWrongDecision,
    matchedFindings: matched.size,
    matchedReportedFindings: usedFindings.size,
    totalFindings: findings.length,
    requiredFindings: required.length,
    matchedExpectations,
    missedExpectations,
    mislocalizedExpectations,
    unmatchedFindings: falsePositiveFindings.map((finding) => finding.title),
    relatedFindings: relatedFindings.map((finding) => finding.title),
    // Per-expectation hit/miss, keyed by flag type — feeds per-rule recall.
    expectationDetails: required.map((expected, index) => ({
      type: expected.type ?? describeFlagExpectation(expected),
      severity: normalize(expected.severity) || null,
      matched: matched.has(index),
    })),
  };
}

// Aggregate blind-agent case scores into the scorecard summary: decision
// accuracy, weighted recall, localization, precision (matched findings over
// all findings the reviewers reported), false-positive total, and recall per
// flag type across every case that required it.
export function summarizeAgentScores(scores) {
  const average = (values) =>
    values.length === 0 ? 0 : values.reduce((sum, value) => sum + value, 0) / values.length;
  const matchedFindings = scores.reduce((sum, score) => sum + (score.matchedFindings ?? 0), 0);
  const matchedReportedFindings = scores.reduce(
    (sum, score) => sum + (score.matchedReportedFindings ?? score.matchedFindings ?? 0),
    0,
  );
  const totalRelatedFindings = scores.reduce((sum, score) => sum + (score.relatedFindings?.length ?? 0), 0);
  const totalFindings = scores.reduce(
    (sum, score) =>
      sum +
      (score.totalFindings ??
        (score.matchedFindings ?? 0) +
          (score.falsePositives ?? 0) +
          (score.relatedFindings?.length ?? 0)),
    0,
  );

  const perRule = new Map();
  for (const score of scores) {
    for (const detail of score.expectationDetails ?? []) {
      const entry = perRule.get(detail.type) ?? { required: 0, matched: 0 };
      entry.required += 1;
      entry.matched += detail.matched ? 1 : 0;
      perRule.set(detail.type, entry);
    }
  }
  const perRuleRecall = Object.fromEntries(
    [...perRule.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([type, entry]) => [
        type,
        { required: entry.required, matched: entry.matched, recall: entry.matched / entry.required },
      ]),
  );

  return {
    decisionAccuracy: average(scores.map((score) => (score.decisionAccepted ? 1 : 0))),
    averageRecall: average(scores.map((score) => score.recall)),
    averageLocalization: average(scores.map((score) => score.localization)),
    // Precision = fraction of reported findings that actually matched a
    // required risk. Near-miss "related" findings did NOT match, so they lower
    // precision (they sit in the denominator, never the numerator).
    precision: totalFindings === 0 ? 1 : matchedReportedFindings / totalFindings,
    matchedFindings,
    matchedReportedFindings,
    totalFindings,
    totalRelatedFindings,
    totalFalsePositives: scores.reduce((sum, score) => sum + (score.falsePositives ?? 0), 0),
    perRuleRecall,
  };
}

export function validateAgentAnswer(answer) {
  if (!answer || typeof answer !== "object") {
    throw new Error("answer must be an object");
  }
  if (!["approve", "investigate", "block"].includes(normalize(answer.decision))) {
    throw new Error("answer.decision must be approve, investigate, or block");
  }
  if (!Array.isArray(answer.findings)) {
    throw new Error("answer.findings must be an array");
  }
  for (const [index, finding] of answer.findings.entries()) {
    if (!finding || typeof finding !== "object") {
      throw new Error(`answer.findings[${index}] must be an object`);
    }
    // The packet prompt requires the full shape; a title-only "finding"
    // cannot claim the cite-evidence-and-location credit the rubric awards.
    for (const field of ["title", "filePath", "riskType", "evidence"]) {
      if (typeof finding[field] !== "string" || finding[field].trim() === "") {
        throw new Error(`answer.findings[${index}].${field} is required`);
      }
    }
    if (!severityScore.has(normalize(finding.severity))) {
      throw new Error(`answer.findings[${index}].severity must be high, medium, or low`);
    }
  }
  // Benchmark v2: benign observations live in `notes`, which scoring ignores
  // entirely (no credit, no penalty) — only its shape is validated.
  if (answer.notes !== undefined) {
    if (!Array.isArray(answer.notes) || answer.notes.some((note) => typeof note !== "string")) {
      throw new Error("answer.notes must be an array of strings when present");
    }
  }
}

export function flagMatches(flag, expected) {
  if (expected.type && flag.type !== expected.type) {
    return false;
  }
  if (expected.severity && normalize(flag.severity) !== normalize(expected.severity)) {
    return false;
  }
  if (expected.filePath && normalize(flag.filePath) !== normalize(expected.filePath)) {
    return false;
  }
  if (expected.nodePath && flag.nodePath !== expected.nodePath) {
    return false;
  }
  if (expected.nodePathIncludes && !String(flag.nodePath ?? "").includes(expected.nodePathIncludes)) {
    return false;
  }
  if (expected.descIncludes && !String(flag.desc ?? "").includes(expected.descIncludes)) {
    return false;
  }
  return true;
}

function bestFindingIndex(findings, usedFindings, expected) {
  const candidates = findings
    .map((finding, index) => ({ finding, index }))
    .filter(({ finding, index }) => !usedFindings.has(index) && findingMatchesExpected(finding, expected));

  const localized = candidates.find(({ finding }) => findingFileMatches(finding, expected));
  return (localized ?? candidates[0])?.index ?? -1;
}

function findingMatchesRisk(finding, expected) {
  const haystack = searchableText(finding.title, finding.riskType, finding.evidence);
  return expected.type ? expectedTerms(expected).some((term) => haystack.includes(term)) : true;
}

function findingMatchesExpected(finding, expected) {
  return findingMatchesRisk(finding, expected) && findingSeverityMatches(finding, expected);
}

function findingSeverityMatches(finding, expected) {
  return expected.severity ? severityWeight(finding.severity) >= severityWeight(expected.severity) : true;
}

function findingFileMatches(finding, expected) {
  return expected.filePath ? normalize(finding.filePath) === normalize(expected.filePath) : true;
}

function findingRelatedToExpected(finding, expected) {
  return findingFileMatches(finding, expected) && findingMatchesRisk(finding, expected);
}

function findingDuplicatesMatchedExpectation(finding, required, matched) {
  return required.some(
    (expected, index) =>
      matched.has(index) &&
      findingFileMatches(finding, expected) &&
      findingMatchesRisk(finding, expected),
  );
}

function topRiskRankedFirst(findings, required) {
  if (required.length === 0) {
    return findings.length === 0;
  }
  if (findings.length === 0) {
    return false;
  }
  const maxWeight = Math.max(...required.map((expected) => severityWeight(expected.severity)));
  const first = findings[0];
  return required
    .filter((expected) => severityWeight(expected.severity) === maxWeight)
    .some((expected) => findingMatchesExpected(first, expected) && findingFileMatches(first, expected));
}

function inferDecision(required) {
  if (required.some((expected) => normalize(expected.severity) === "high")) {
    return "block";
  }
  if (required.length > 0) {
    return "investigate";
  }
  return "approve";
}

function acceptedDecisionSet(caseDef, expectedDecision) {
  const accepted = caseDef.agent?.acceptedDecisions ?? [expectedDecision];
  return accepted.map(normalize);
}

function severityWeight(severity) {
  return severityScore.get(normalize(severity)) ?? 1;
}

function expectEqual(failures, label, actual, expected) {
  if (expected !== undefined && actual !== expected) {
    failures.push(`${label}: expected ${expected}, got ${actual}`);
  }
}

function describeFlagExpectation(expected) {
  return [expected.severity, expected.type, expected.filePath, expected.nodePath ?? expected.nodePathIncludes]
    .filter(Boolean)
    .join(" / ");
}

function normalize(value) {
  return String(value ?? "")
    .replace(/\\/g, "/")
    .trim()
    .toLowerCase();
}

function searchableText(...values) {
  return values.map(canonicalText).join(" ");
}

function canonicalText(value) {
  return normalize(value).replace(/[^a-z0-9/._]+/g, " ").replace(/\s+/g, " ").trim();
}

function expectedTerms(expected) {
  return [expected.type, ...(expected.aliases ?? []), ...(flagAliases.get(expected.type) ?? [])]
    .filter(Boolean)
    .map(canonicalText);
}
