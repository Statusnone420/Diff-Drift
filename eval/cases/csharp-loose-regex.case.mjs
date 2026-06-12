// An email-validation regex field is widened to a catch-all, so any string now
// passes validation.
export default {
  id: "csharp-loose-regex",
  title: "C# validation regex widened to a catch-all",
  repo: {
    project: "signup-svc",
    branch: "agent/relax-email-check",
  },
  before: {
    "src/Validators.cs": `public class Validators {
    private static readonly Regex EmailRe = new Regex("^[^@]+@[^@]+\\\\.[^@]+$");

    public bool IsEmail(string value) {
        return EmailRe.IsMatch(value);
    }
}
`,
  },
  after: {
    "src/Validators.cs": `public class Validators {
    private static readonly Regex EmailRe = new Regex(".*");

    public bool IsEmail(string value) {
        return EmailRe.IsMatch(value);
    }
}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Loose regex pattern", severity: "high", filePath: "src/Validators.cs" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
