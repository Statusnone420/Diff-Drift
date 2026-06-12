// An input sanitization call is dropped between reading user input and storing
// it. The stored value is now raw — a classic injection-exposure regression.
export default {
  id: "csharp-removed-sanitize",
  title: "C# sanitization call removed before storing input",
  repo: {
    project: "comments-svc",
    branch: "agent/streamline-store",
  },
  before: {
    "src/CommentStore.cs": `public class CommentStore {
    public void Save(string input) {
        var clean = sanitizer.escape(input);
        Persist(clean);
    }
}
`,
  },
  after: {
    "src/CommentStore.cs": `public class CommentStore {
    public void Save(string input) {
        Persist(input);
    }
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Removed sanitization", severity: "low", filePath: "src/CommentStore.cs" }],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
