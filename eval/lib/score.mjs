const severityScore = new Map([
  ["low", 1],
  ["medium", 2],
  ["high", 3],
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
  let weightedHit = 0;
  let weightedTotal = 0;
  let localized = 0;

  required.forEach((expected, index) => {
    const weight = severityWeight(expected.severity);
    weightedTotal += weight;
    const finding = findings.find((candidate) => findingMatches(candidate, expected));
    if (finding) {
      matched.add(index);
      weightedHit += weight;
      if (!expected.filePath || normalize(finding.filePath) === normalize(expected.filePath)) {
        localized += 1;
      }
    }
  });

  const falsePositives = findings.filter(
    (finding) => !required.some((expected) => findingMatches(finding, expected)),
  ).length;
  const expectedDecision = caseDef.agent?.expectedDecision ?? inferDecision(required);
  const decisionCorrect = normalize(answer.decision) === expectedDecision;
  const recall = weightedTotal === 0 ? (findings.length === 0 ? 1 : 0) : weightedHit / weightedTotal;
  const localization = matched.size === 0 ? (required.length === 0 ? 1 : 0) : localized / matched.size;
  const topRisk = topRiskRankedFirst(findings, required);
  const rawScore =
    recall * 60 +
    (decisionCorrect ? 20 : 0) +
    (topRisk ? 10 : 0) +
    localization * 10 -
    falsePositives * 5;

  return {
    caseId: caseDef.id,
    expectedDecision,
    decisionCorrect,
    score: Math.max(0, Math.round(rawScore)),
    recall,
    localization,
    falsePositives,
    topRisk,
    matchedFindings: matched.size,
    requiredFindings: required.length,
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
    if (typeof finding.title !== "string" || finding.title.trim() === "") {
      throw new Error(`answer.findings[${index}].title is required`);
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

function findingMatches(finding, expected) {
  const haystack = `${finding.title ?? ""} ${finding.riskType ?? ""} ${finding.evidence ?? ""}`.toLowerCase();
  const typeMatch = expected.type ? haystack.includes(expected.type.toLowerCase()) : true;
  const fileMatch = expected.filePath ? normalize(finding.filePath) === normalize(expected.filePath) : true;
  return typeMatch && fileMatch;
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
    .some((expected) => findingMatches(first, expected));
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
