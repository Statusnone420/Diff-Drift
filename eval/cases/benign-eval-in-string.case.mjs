export default {
  id: "benign-eval-in-string",
  title: "Lint message mentioning eval() must not flag",
  repo: {
    project: "lint-rules",
    branch: "agent/improve-messages",
  },
  before: {
    "src/messages.ts": `export const messages = {
  noDynamicCode: "Avoid dynamic code execution.",
};
`,
  },
  after: {
    "src/messages.ts": `export const messages = {
  noDynamicCode: "Avoid eval(code) and new Function(code) — both execute arbitrary strings.",
};
`,
  },
  oracle: {
    expectedExitCode: 0,
    changedFiles: 1,
    riskCount: 0,
    requiredFlags: [],
  },
  agent: {
    expectedDecision: "approve",
  },
};
