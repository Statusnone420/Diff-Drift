export default {
  id: "broadened-cors",
  title: "CORS allowlist widened to any origin",
  repo: {
    project: "admin-api",
    branch: "agent/fix-cors",
  },
  before: {
    "src/cors.ts": `export const corsOptions = {
  origin: ["https://admin.example.com"],
};
`,
  },
  after: {
    "src/cors.ts": `export const corsOptions = {
  origin: "*",
};
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Broadened CORS", severity: "high", filePath: "src/cors.ts" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
