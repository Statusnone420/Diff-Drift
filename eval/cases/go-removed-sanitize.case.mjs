// Go removed-sanitize: an input-sanitization wrapper is stripped, so raw user
// input flows straight into the persistence call. The before code wraps the
// comment in `sanitizeHTML(...)`; the after passes it through unescaped.
export default {
  id: "go-removed-sanitize",
  title: "Go comment store drops its sanitizer",
  repo: {
    project: "comments-go",
    branch: "agent/refactor-store",
  },
  before: {
    "store.go": `package comments

func Store(c Comment) error {
    return db.Insert(sanitizeHTML(c.Body))
}
`,
  },
  after: {
    "store.go": `package comments

func Store(c Comment) error {
    return db.Insert(c.Body)
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Removed sanitization", severity: "low", filePath: "store.go" },
    ],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
