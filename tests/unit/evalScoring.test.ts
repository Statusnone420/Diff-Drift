// @ts-nocheck
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
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
    expect(score.recall).toBe(1);
  });

  it("rejects malformed answers", () => {
    expect(() => validateAgentAnswer({ decision: "ship", findings: [] })).toThrow(
      "answer.decision",
    );
    expect(() => validateAgentAnswer({ decision: "approve" })).toThrow("answer.findings");
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
