// Go loose-regex: a validation pattern loses its anchors, so a string that
// merely contains a valid-looking substring now passes. The before requires the
// whole input to be the token shape; the after matches it anywhere.
export default {
  id: "go-loose-regex",
  title: "Go token validator loses its anchors",
  repo: {
    project: "tokens-go",
    branch: "agent/relax-token-regex",
  },
  before: {
    "token.go": `package tokens

var tokenRe = regexp.MustCompile(` + "`^[A-Za-z0-9]{32}$`" + `)
`,
  },
  after: {
    "token.go": `package tokens

var tokenRe = regexp.MustCompile(` + "`[A-Za-z0-9]{32}`" + `)
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Loose regex pattern", severity: "high", filePath: "token.go" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
