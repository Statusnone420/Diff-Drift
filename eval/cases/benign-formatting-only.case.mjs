export default {
  id: "benign-formatting-only",
  title: "Formatting-only route cleanup",
  repo: {
    project: "profile-api",
    branch: "agent/format-route",
  },
  before: {
    "src/routes/profile.ts": `export function getProfile(id: string) {
  return { id, active: true };
}
`,
  },
  after: {
    "src/routes/profile.ts": `export function getProfile(id: string) {
    return { id, active: true };
}
`,
  },
  oracle: {
    expectedExitCode: 0,
    changedFiles: 1,
    riskCount: 0,
    requiredFlags: [],
    forbiddenFlags: [{ severity: "high" }, { severity: "medium" }, { severity: "low" }],
    files: [{ path: "src/routes/profile.ts", summary: "Formatting only", risks: 0 }],
  },
  agent: {
    expectedDecision: "approve",
  },
};
