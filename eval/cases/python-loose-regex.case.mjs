// An anchored email validation pattern is widened to a catch-all, so any string
// now passes the check that used to enforce a real shape.
export default {
  id: "python-loose-regex",
  title: "Email validation regex widened to a catch-all",
  repo: {
    project: "signup-svc",
    branch: "agent/relax-email-check",
  },
  before: {
    "signup/validate.py": `EMAIL_RE = re.compile("^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$")
`,
  },
  after: {
    "signup/validate.py": `EMAIL_RE = re.compile(".*")
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Loose regex pattern", severity: "high", filePath: "signup/validate.py" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
