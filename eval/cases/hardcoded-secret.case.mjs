export default {
  id: "hardcoded-secret",
  title: "API key committed in source",
  repo: {
    project: "integrations-api",
    branch: "agent/add-provider-config",
  },
  before: {
    "src/provider.ts": `export const provider = {
  endpoint: "https://api.example.com",
};
`,
  },
  after: {
    "src/provider.ts": `export const provider = {
  endpoint: "https://api.example.com",
  apiKey: "sk-abcdefghijklmnop",
};
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Hardcoded secret", severity: "high", filePath: "src/provider.ts" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
