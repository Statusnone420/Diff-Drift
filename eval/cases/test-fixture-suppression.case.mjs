export default {
  id: "test-fixture-suppression",
  title: "Hardcoded-looking test fixture stays quiet",
  repo: {
    project: "fixture-heavy-api",
    branch: "agent/add-test-fixture",
  },
  before: {
    "src/index.ts": `export const ok = true;
`,
  },
  after: {
    "src/index.ts": `export const ok = true;
`,
    "tests/fixtures/provider.test.ts": `export const fakeAwsKey = "AKIA0123456789ABCDEF";
`,
  },
  oracle: {
    expectedExitCode: 0,
    changedFiles: 1,
    riskCount: 0,
    requiredFlags: [],
    forbiddenFlags: [{ type: "Hardcoded secret" }],
  },
  agent: {
    expectedDecision: "approve",
  },
};
