// @ts-nocheck
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { diffDriftCommand } from "../../eval/lib/cli.mjs";
import { renderAgentDashboard, renderAgentScorecard } from "../../eval/lib/packets.mjs";
import { createCaseRepo } from "../../eval/lib/repo.mjs";
import { scoreAgentAnswer, scoreEngineResult, validateAgentAnswer } from "../../eval/lib/score.mjs";

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
            filePath: "package.json",
            riskType: "Dependency drift / lockfile inconsistency",
            evidence: "ghost-payments-sdk is not in the lockfile",
          },
          {
            title: "New install-time script",
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
            filePath: "src/cookies.ts",
            riskType: "weakened-cookie-flags",
            evidence: "httpOnly removed",
          },
          {
            title: "Refresh cookie lost Secure",
            filePath: "src/cookies.ts",
            riskType: "weakened-cookie-flags",
            evidence: "secure removed",
          },
          {
            title: "CSRF cookie downgraded SameSite",
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
    expect(html).toContain("Score distribution");
    expect(html).toContain("synthetic-risk");
  });

  it("rejects malformed answers", () => {
    expect(() => validateAgentAnswer({ decision: "ship", findings: [] })).toThrow(
      "answer.decision",
    );
    expect(() => validateAgentAnswer({ decision: "approve" })).toThrow("answer.findings");
  });
});

describe("eval CLI command selection", () => {
  it("builds the current checkout unless an eval binary is explicitly configured", () => {
    const previous = process.env.DIFF_DRIFT_EVAL_BIN;
    delete process.env.DIFF_DRIFT_EVAL_BIN;

    try {
      const command = diffDriftCommand(["check", "repo", "--json"]);
      expect(command.bin).toBe("cargo");
      expect(command.args).toEqual([
        "run",
        "--quiet",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--",
        "check",
        "repo",
        "--json",
      ]);

      process.env.DIFF_DRIFT_EVAL_BIN = "custom-diff-drift";
      expect(diffDriftCommand(["check"]).bin).toBe("custom-diff-drift");
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
