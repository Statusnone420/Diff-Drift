export default {
  id: "deps-python-requirements-new",
  title: "New pip dependency flagged at low (no lockfile claim)",
  repo: {
    project: "py-api",
    branch: "agent/add-auth",
  },
  before: {
    "requirements.txt": `flask==2.3.0
requests>=2.28
`,
    "app.py": `print("hi")
`,
  },
  after: {
    "requirements.txt": `flask==2.3.0
requests>=2.28
jwt-helper-lite==0.0.1
`,
    "app.py": `print("hi")
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "New dependency", severity: "low", filePath: "requirements.txt" },
    ],
  },
  agent: {
    expectedDecision: "investigate",
    acceptedDecisions: ["approve", "investigate"],
  },
};
