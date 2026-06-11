export default {
  id: "test-file-hardcoded-secret",
  title: "Hardcoded secret in a test fixture is still flagged",
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
  // Secrets are flagged everywhere, including test paths: a real key pasted into
  // a fixture is still a leak, and the AWS/OpenAI/PEM markers are specific enough
  // to stay low-noise. (Noisier rules — child_process, eval, TLS — still suppress
  // in test files; this rule deliberately does not.)
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      {
        type: "Hardcoded secret",
        severity: "high",
        filePath: "tests/fixtures/provider.test.ts",
      },
    ],
    forbiddenFlags: [],
  },
  agent: {
    expectedDecision: "block",
    acceptedDecisions: ["block", "investigate"],
  },
};
