export default {
  id: "disabled-tls-verification",
  title: "HTTPS agent disables certificate validation",
  repo: {
    project: "webhook-relay",
    branch: "agent/fix-staging-webhooks",
  },
  before: {
    "src/httpClient.ts": `export const agentOptions = {
  keepAlive: true,
};
`,
  },
  after: {
    "src/httpClient.ts": `export const agentOptions = {
  keepAlive: true,
  rejectUnauthorized: false,
};
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Disabled TLS verification", severity: "high", filePath: "src/httpClient.ts" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
