// @ts-nocheck
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { diffDriftCommand, diffDriftRuntimeEnv } from "../../eval/lib/cli.mjs";
import { renderAgentDashboard, renderAgentScorecard } from "../../eval/lib/packets.mjs";
import { createCaseRepo } from "../../eval/lib/repo.mjs";
import {
  scoreAgentAnswer,
  scoreEngineResult,
  summarizeAgentScores,
  validateAgentAnswer,
} from "../../eval/lib/score.mjs";

const caseDef = {
  id: "synthetic-risk",
  title: "Synthetic risk",
  before: {},
  after: {},
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Loose regex pattern", severity: "high", filePath: "src/auth.ts" }],
    forbiddenFlags: [{ type: "Hardcoded secret" }],
    files: [{ path: "src/auth.ts", summary: "1 modified", risks: 1 }],
  },
  agent: { expectedDecision: "block" },
};

const run = {
  exitCode: 3,
  data: {
    session: { changedFiles: 1, riskCount: 1, fileCount: 1 },
    flags: [
      {
        type: "Loose regex pattern",
        severity: "high",
        filePath: "src/auth.ts",
        nodePath: "validateToken > pattern",
        dismissed: false,
      },
    ],
    files: [{ dir: "src/", name: "auth.ts", summary: "1 modified", risks: 1 }],
  },
};

describe("eval engine scoring", () => {
  it("passes when CLI output matches the oracle", () => {
    const score = scoreEngineResult(caseDef, run);
    expect(score.passed).toBe(true);
    expect(score.failures).toEqual([]);
    expect(score.stats.requiredFlags).toBe(1);
  });

  it("reports missing and forbidden flags", () => {
    const score = scoreEngineResult(caseDef, {
      ...run,
      data: {
        ...run.data,
        flags: [{ type: "Hardcoded secret", severity: "high", filePath: "src/auth.ts", dismissed: false }],
      },
    });

    expect(score.passed).toBe(false);
    expect(score.failures.join("\n")).toContain("missing flag");
    expect(score.failures.join("\n")).toContain("forbidden flag present");
  });
});

describe("blind-agent answer scoring", () => {
  it("scores a useful blind answer by recall, decision, top risk, and localization", () => {
    const score = scoreAgentAnswer(caseDef, {
      caseId: "synthetic-risk",
      decision: "block",
      confidence: 0.9,
      findings: [
        {
          title: "Loose regex pattern disables validation",
          severity: "high",
          filePath: "src/auth.ts",
          riskType: "Loose regex pattern",
          evidence: "pattern changed to /.*/",
        },
      ],
    });

    expect(score.score).toBe(100);
    expect(score.decisionCorrect).toBe(true);
    expect(score.decisionAccepted).toBe(true);
    expect(score.recall).toBe(1);
  });

  it("accepts calibrated decisions and semantic flag aliases", () => {
    const score = scoreAgentAnswer(
      {
        ...caseDef,
        oracle: {
          ...caseDef.oracle,
          requiredFlags: [
            { type: "Dependency not in lockfile", severity: "high", filePath: "package.json" },
            { type: "npm script changed", severity: "medium", filePath: "package.json" },
          ],
        },
        agent: { expectedDecision: "block", acceptedDecisions: ["investigate", "block"] },
      },
      {
        decision: "investigate",
        findings: [
          {
            title: "Dependency added without lockfile entry",
            severity: "high",
            filePath: "package.json",
            riskType: "Dependency drift / lockfile inconsistency",
            evidence: "ghost-payments-sdk is not in the lockfile",
          },
          {
            title: "New install-time script",
            severity: "medium",
            filePath: "package.json",
            riskType: "npm script drift",
            evidence: "postinstall runs node scripts/bootstrap.js",
          },
        ],
      },
    );

    expect(score.decisionAccepted).toBe(true);
    expect(score.score).toBe(100);
    expect(score.recall).toBe(1);
  });

  it("matches duplicate same-type findings with distinct answer findings", () => {
    const score = scoreAgentAnswer(
      {
        ...caseDef,
        oracle: {
          ...caseDef.oracle,
          requiredFlags: [
            { type: "Weakened cookie flags", severity: "high", filePath: "src/cookies.ts" },
            { type: "Weakened cookie flags", severity: "high", filePath: "src/cookies.ts" },
            { type: "Weakened cookie flags", severity: "high", filePath: "src/cookies.ts" },
          ],
        },
      },
      {
        decision: "block",
        findings: [
          {
            title: "Access cookie lost HttpOnly",
            severity: "high",
            filePath: "src/cookies.ts",
            riskType: "weakened-cookie-flags",
            evidence: "httpOnly removed",
          },
          {
            title: "Refresh cookie lost Secure",
            severity: "high",
            filePath: "src/cookies.ts",
            riskType: "weakened-cookie-flags",
            evidence: "secure removed",
          },
          {
            title: "CSRF cookie downgraded SameSite",
            severity: "high",
            filePath: "src/cookies.ts",
            riskType: "weakened-cookie-flags",
            evidence: "sameSite Strict became None",
          },
        ],
      },
    );

    expect(score.score).toBe(100);
    expect(score.matchedFindings).toBe(3);
    expect(score.falsePositives).toBe(0);
  });

  it("does not let one finding satisfy duplicate same-type expectations", () => {
    const score = scoreAgentAnswer(
      {
        ...caseDef,
        oracle: {
          ...caseDef.oracle,
          requiredFlags: [
            { type: "Weakened cookie flags", severity: "high", filePath: "src/cookies.ts" },
            { type: "Weakened cookie flags", severity: "high", filePath: "src/cookies.ts" },
          ],
        },
      },
      {
        decision: "block",
        findings: [
          {
            title: "Session cookies lost hardening",
            severity: "high",
            filePath: "src/cookies.ts",
            riskType: "weakened-cookie-flags",
            evidence: "httpOnly and secure were removed from cookie options.",
          },
        ],
      },
    );

    expect(score.matchedFindings).toBe(1);
    expect(score.requiredFindings).toBe(2);
    expect(score.missedExpectations).toEqual([
      "high / Weakened cookie flags / src/cookies.ts",
    ]);
    expect(score.recall).toBe(0.5);
  });

  it("lets one compound finding satisfy distinct risk types", () => {
    const score = scoreAgentAnswer(
      {
        ...caseDef,
        oracle: {
          ...caseDef.oracle,
          requiredFlags: [
            { type: "Loose regex pattern", severity: "high", filePath: "src/auth.ts" },
            { type: "Disabled guard", severity: "low", filePath: "src/auth.ts" },
          ],
        },
      },
      {
        decision: "block",
        findings: [
          {
            title: "Validation accepts every token",
            severity: "high",
            filePath: "src/auth.ts",
            riskType: "Loose regex pattern / Disabled guard",
            evidence: "`/.*/` accepts any string and `if (false)` makes the rejection branch unreachable.",
          },
        ],
      },
    );

    expect(score.matchedFindings).toBe(2);
    expect(score.requiredFindings).toBe(2);
    expect(score.missedExpectations).toEqual([]);
    expect(score.recall).toBe(1);
    expect(score.score).toBe(100);
  });

  it("penalizes always-block decisions on benign cases", () => {
    const score = scoreAgentAnswer(
      {
        ...caseDef,
        oracle: {
          ...caseDef.oracle,
          requiredFlags: [],
        },
        agent: { expectedDecision: "approve" },
      },
      {
        decision: "block",
        findings: [],
      },
    );

    expect(score.benignWrongDecision).toBe(true);
    expect(score.decisionAccepted).toBe(false);
    expect(score.score).toBeLessThan(50);
  });

  it("separates risk recall from wrong-file localization", () => {
    const score = scoreAgentAnswer(caseDef, {
      decision: "block",
      findings: [
        {
          title: "Loose regex pattern disables validation",
          severity: "high",
          filePath: "src/wrong.ts",
          riskType: "Loose regex pattern",
          evidence: "pattern changed to /.*/",
        },
      ],
    });

    expect(score.recall).toBe(1);
    expect(score.localization).toBe(0);
    expect(score.matchedFindings).toBe(1);
    expect(score.missedExpectations).toEqual([]);
    expect(score.mislocalizedExpectations).toEqual(["high / Loose regex pattern / src/auth.ts"]);
    expect(score.score).toBe(90);
  });

  it("rejects severity understatements", () => {
    const score = scoreAgentAnswer(caseDef, {
      decision: "block",
      findings: [
        {
          title: "Loose regex pattern disables validation",
          severity: "low",
          filePath: "src/auth.ts",
          riskType: "Loose regex pattern",
          evidence: "pattern changed to /.*/",
        },
      ],
    });

    expect(score.recall).toBe(0);
    expect(score.localization).toBe(0);
    expect(score.topRisk).toBe(false);
    expect(score.matchedFindings).toBe(0);
    expect(score.falsePositives).toBe(0);
    expect(score.missedExpectations).toEqual(["high / Loose regex pattern / src/auth.ts"]);
    expect(score.unmatchedFindings).toEqual([]);
    expect(score.relatedFindings).toEqual(["Loose regex pattern disables validation"]);
    expect(score.score).toBe(20);
  });

  it("allows conservative severity escalation", () => {
    const score = scoreAgentAnswer(
      {
        ...caseDef,
        oracle: {
          ...caseDef.oracle,
          requiredFlags: [{ type: "Removed sanitization", severity: "low", filePath: "src/comments.ts" }],
        },
        agent: { expectedDecision: "investigate" },
      },
      {
        decision: "investigate",
        findings: [
          {
            title: "Sanitization removed from user-controlled HTML",
            severity: "high",
            filePath: "src/comments.ts",
            riskType: "Removed sanitization",
            evidence: "sanitizeHtml(body) was removed before rendering the comment body.",
          },
        ],
      },
    );

    expect(score.recall).toBe(1);
    expect(score.localization).toBe(1);
    expect(score.matchedFindings).toBe(1);
    expect(score.falsePositives).toBe(0);
    expect(score.score).toBe(100);
  });

  it("does not let severity escalation make a lower-priority risk the top risk", () => {
    const score = scoreAgentAnswer(
      {
        ...caseDef,
        oracle: {
          ...caseDef.oracle,
          requiredFlags: [
            { type: "Loose regex pattern", severity: "high", filePath: "src/auth.ts" },
            { type: "Removed sanitization", severity: "low", filePath: "src/comments.ts" },
          ],
        },
      },
      {
        decision: "block",
        findings: [
          {
            title: "Sanitization removed from user-controlled HTML",
            severity: "high",
            filePath: "src/comments.ts",
            riskType: "Removed sanitization",
            evidence: "sanitizeHtml(body) was removed before rendering the comment body.",
          },
          {
            title: "Loose regex pattern disables validation",
            severity: "high",
            filePath: "src/auth.ts",
            riskType: "Loose regex pattern",
            evidence: "pattern changed to /.*/",
          },
        ],
      },
    );

    expect(score.recall).toBe(1);
    expect(score.topRisk).toBe(false);
    expect(score.score).toBe(90);
  });

  it("summarizes precision, false positives, and per-rule recall across cases", () => {
    const hit = scoreAgentAnswer(caseDef, {
      decision: "block",
      findings: [
        {
          title: "Loose regex pattern disables validation",
          severity: "high",
          filePath: "src/auth.ts",
          riskType: "Loose regex pattern",
          evidence: "pattern changed to /.*/",
        },
        {
          title: "Suspicious but unrelated observation",
          severity: "low",
          filePath: "src/other.ts",
          riskType: "Speculation",
          evidence: "not an expected flag",
        },
      ],
    });
    const miss = scoreAgentAnswer(caseDef, { decision: "block", findings: [] });

    const summary = summarizeAgentScores([hit, miss]);
    expect(summary.matchedFindings).toBe(1);
    expect(summary.totalFindings).toBe(2);
    expect(summary.precision).toBe(0.5);
    expect(summary.totalFalsePositives).toBe(1);
    expect(summary.perRuleRecall["Loose regex pattern"]).toEqual({
      required: 2,
      matched: 1,
      recall: 0.5,
    });
  });

  it("tracks related extra findings without counting them as false positives", () => {
    const score = scoreAgentAnswer(caseDef, {
      decision: "block",
      findings: [
        {
          title: "Loose regex pattern disables validation",
          severity: "high",
          filePath: "src/auth.ts",
          riskType: "Loose regex pattern",
          evidence: "pattern changed to /.*/",
        },
        {
          title: "Validation now accepts arbitrary input",
          severity: "high",
          filePath: "src/auth.ts",
          riskType: "Validation regression",
          evidence: "The loose regex pattern accepts any token string.",
        },
      ],
    });

    expect(score.falsePositives).toBe(0);
    expect(score.relatedFindings).toEqual(["Validation now accepts arbitrary input"]);

    const summary = summarizeAgentScores([score]);
    expect(summary.precision).toBe(1);
    expect(summary.matchedReportedFindings).toBe(1);
    expect(summary.totalRelatedFindings).toBe(1);
    expect(summary.totalFalsePositives).toBe(0);
  });

  it("treats a clean benign run as perfect precision", () => {
    const summary = summarizeAgentScores([
      scoreAgentAnswer(
        { ...caseDef, oracle: { ...caseDef.oracle, requiredFlags: [] }, agent: { expectedDecision: "approve" } },
        { decision: "approve", findings: [] },
      ),
    ]);
    expect(summary.precision).toBe(1);
    expect(summary.totalFindings).toBe(0);
    expect(summary.perRuleRecall).toEqual({});
  });

  it("renders advisory scorecards separate from CI gating", () => {
    const result = {
      generatedAt: "2026-06-10T00:00:00.000Z",
      averageScore: 95,
      summary: { decisionAccuracy: 1, averageRecall: 0.95, averageLocalization: 1 },
      scores: [
        {
          caseId: "synthetic-risk",
          score: 95,
          decisionAccepted: true,
          acceptedDecisions: ["block"],
          recall: 0.95,
          localization: 1,
          falsePositives: 0,
          matchedFindings: 1,
          requiredFindings: 2,
          missedExpectations: ["medium / Undeclared import / src/auth.ts"],
        },
      ],
    };

    const md = renderAgentScorecard(result);
    const html = renderAgentDashboard(result);
    expect(md).toContain("Advisory only");
    expect(md).toContain("Overall score");
    expect(md).toContain("missed Undeclared import");
    expect(html).toContain("Blind-agent scorecard");
    expect(html).toContain("Case score histogram");
    expect(html).toContain("synthetic-risk");
  });

  it("labels evaluators and surfaces the external-validation banner", () => {
    const result = {
      generatedAt: "2026-06-10T00:00:00.000Z",
      averageScore: 95,
      summary: {
        decisionAccuracy: 1,
        averageRecall: 0.95,
        averageLocalization: 1,
        precision: 0.9,
        matchedFindings: 9,
        totalFindings: 10,
        totalFalsePositives: 1,
        perRuleRecall: {
          "Loose regex pattern": { required: 2, matched: 2, recall: 1 },
          "Undeclared import": { required: 2, matched: 1, recall: 0.5 },
        },
      },
      evaluators: [{ id: "claude-fable-5", kind: "model", cases: 10, averageScore: 95 }],
      externalValidationPending: true,
      scores: [],
    };

    const md = renderAgentScorecard(result);
    expect(md).toContain("claude-fable-5 (model, 10 cases)");
    expect(md).toContain("Independent external validation pending");
    expect(md).toContain("Per-rule recall");
    expect(md).toContain("| Undeclared import | 1/2 | 50% |");
    expect(md).toContain("Precision: 90%");

    const html = renderAgentDashboard(result);
    expect(html).toContain("Independent external validation pending");
    expect(html).toContain("Precision");
    expect(html).toContain("claude-fable-5 (model, 10)");
  });

  it("rejects malformed answers", () => {
    expect(() => validateAgentAnswer({ decision: "ship", findings: [] })).toThrow(
      "answer.decision",
    );
    expect(() => validateAgentAnswer({ decision: "approve" })).toThrow("answer.findings");
  });

  it("enforces the full finding shape the packet prompt requires", () => {
    // The prompt asks for severity, filePath, riskType, and evidence; a
    // title-only finding must not be scoreable.
    const full = {
      title: "Loose regex pattern disables validation",
      severity: "high",
      filePath: "src/auth.ts",
      riskType: "Loose regex pattern",
      evidence: "pattern changed to /.*/",
    };
    expect(() =>
      validateAgentAnswer({ decision: "block", findings: [full] }),
    ).not.toThrow();

    for (const missing of ["severity", "filePath", "riskType", "evidence"] as const) {
      const { [missing]: _omitted, ...partial } = full;
      expect(() => validateAgentAnswer({ decision: "block", findings: [partial] })).toThrow(
        `answer.findings[0].${missing}`,
      );
    }
    expect(() =>
      validateAgentAnswer({ decision: "block", findings: [{ ...full, severity: "fatal" }] }),
    ).toThrow("answer.findings[0].severity");
  });

  it("accepts scoring-ignored notes and validates their shape", () => {
    // Notes carry benign observations without costing precision or recall.
    const benign = {
      ...caseDef,
      oracle: { ...caseDef.oracle, requiredFlags: [] },
      agent: { expectedDecision: "approve" },
    };
    const score = scoreAgentAnswer(benign, {
      decision: "approve",
      findings: [],
      notes: ["Formatting-only change; report matches the raw diff."],
    });
    expect(score.score).toBe(100);
    expect(score.falsePositives).toBe(0);

    expect(() =>
      validateAgentAnswer({ decision: "approve", findings: [], notes: "not an array" }),
    ).toThrow("answer.notes");
    expect(() =>
      validateAgentAnswer({ decision: "approve", findings: [], notes: [42] }),
    ).toThrow("answer.notes");
  });
});

describe("answer file collection", () => {
  it("expands a directory argument into its sorted .json answers", async () => {
    const { collectAnswerFiles } = await import("../../eval/lib/answers.mjs");
    const { mkdtempSync, writeFileSync, rmSync } = await import("node:fs");
    const { join } = await import("node:path");
    const { tmpdir } = await import("node:os");

    const dir = mkdtempSync(join(tmpdir(), "drift-answers-"));
    try {
      writeFileSync(join(dir, "b-case.json"), "{}");
      writeFileSync(join(dir, "a-case.json"), "{}");
      writeFileSync(join(dir, "notes.txt"), "ignored");

      // A directory arg expands to its .json files, sorted.
      const fromDir = collectAnswerFiles([dir], dir);
      expect(fromDir).toEqual([join(dir, "a-case.json"), join(dir, "b-case.json")]);

      // A file arg passes through; mixing works.
      const single = collectAnswerFiles([join(dir, "b-case.json")], dir);
      expect(single).toEqual([join(dir, "b-case.json")]);

      // No args falls back to the default directory.
      expect(collectAnswerFiles([], dir)).toEqual([
        join(dir, "a-case.json"),
        join(dir, "b-case.json"),
      ]);

      // A missing default directory yields an empty list, not a throw.
      expect(collectAnswerFiles([], join(dir, "missing"))).toEqual([]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("eval CLI command selection", () => {
  it("builds the current checkout before running an isolated binary", () => {
    const previous = process.env.DIFF_DRIFT_EVAL_BIN;
    delete process.env.DIFF_DRIFT_EVAL_BIN;

    try {
      const command = diffDriftCommand(["check", "repo", "--json"]);
      expect(command.bin.replace(/\\/g, "/")).toMatch(/src-tauri\/target\/debug\/diff-drift(\.exe)?$/);
      expect(command.args).toEqual(["check", "repo", "--json"]);
      expect(command.build).toEqual({
        bin: "cargo",
        args: [
          "build",
          "--quiet",
          "--manifest-path",
          "src-tauri/Cargo.toml",
          "--bin",
          "diff-drift",
        ],
      });

      const runtimeEnv = diffDriftRuntimeEnv("state-home");
      expect(runtimeEnv.HOME).toBe("state-home");
      expect(runtimeEnv.APPDATA).toBe("state-home");
      expect(runtimeEnv.XDG_CONFIG_HOME).toBe("state-home");

      process.env.DIFF_DRIFT_EVAL_BIN = "custom-diff-drift";
      expect(diffDriftCommand(["check"])).toEqual({ bin: "custom-diff-drift", args: ["check"] });
    } finally {
      if (previous === undefined) {
        delete process.env.DIFF_DRIFT_EVAL_BIN;
      } else {
        process.env.DIFF_DRIFT_EVAL_BIN = previous;
      }
    }
  });
});

describe("eval temp repo generation", () => {
  it("commits the baseline and leaves after-state drift in the working tree", () => {
    const fixture = createCaseRepo({
      id: "unit-temp-repo",
      title: "Unit temp repo",
      repo: { project: "unit-temp-repo", branch: "agent/unit" },
      before: { "src/a.ts": "export const value = 1;\n" },
      after: { "src/a.ts": "export const value = 2;\n" },
      oracle: { expectedExitCode: 0 },
    });

    try {
      const branch = execFileSync("git", ["-C", fixture.repoPath, "branch", "--show-current"], {
        encoding: "utf8",
      }).trim();
      const status = execFileSync("git", ["-C", fixture.repoPath, "status", "--short"], {
        encoding: "utf8",
      }).trim();

      expect(branch).toBe("agent/unit");
      expect(status).toBe("M src/a.ts");
    } finally {
      const root = fixture.tempRoot;
      fixture.cleanup();
      expect(existsSync(root)).toBe(false);
    }
  });
});
