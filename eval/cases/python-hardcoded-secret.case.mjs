export default {
  id: "python-hardcoded-secret",
  title: "AWS key committed in a Python settings module",
  repo: {
    project: "billing-svc",
    branch: "agent/add-aws-client",
  },
  before: {
    "app/settings.py": `def aws_config():
    return {
        "region": "us-east-1",
    }
`,
  },
  after: {
    "app/settings.py": `def aws_config():
    return {
        "region": "us-east-1",
        "access_key": "AKIAIOSFODNN7EXAMPLE",
    }
`,
  },
  oracle: {
    // The one rule that runs cross-language: a hardcoded secret is a
    // language-neutral text marker, so it flags in a .py file just like in TS.
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Hardcoded secret", severity: "high", filePath: "app/settings.py" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
