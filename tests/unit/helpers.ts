import type { Session } from "../../src/types";

/** A loaded HEAD-baseline session with no drift; override per test. */
export function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    project: "payments-api",
    branch: "agent/refactor-token-validation",
    repoPath: "C:/repos/payments-api",
    baselineSpec: "head",
    baselineLabel: "HEAD",
    changedFiles: 0,
    riskCount: 0,
    fileCount: 0,
    changedNodes: 0,
    reviewedNodes: 0,
    approved: false,
    ...overrides,
  };
}
