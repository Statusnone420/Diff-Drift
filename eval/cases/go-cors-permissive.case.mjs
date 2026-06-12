// Go cors-permissive: cross-origin access is opened to any site. The before
// pins an allowlist of one origin; the after sets AllowAllOrigins: true, so
// any website can make credentialed requests against the API.
export default {
  id: "go-cors-permissive",
  title: "Go API opens CORS to any origin",
  repo: {
    project: "api-go",
    branch: "agent/fix-cors",
  },
  before: {
    "cors.go": `package api

func corsConfig() cors.Config {
    cfg := cors.Config{AllowOrigins: []string{"https://app.example.com"}}
    return cfg
}
`,
  },
  after: {
    "cors.go": `package api

func corsConfig() cors.Config {
    cfg := cors.Config{AllowAllOrigins: true}
    return cfg
}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Broadened CORS", severity: "high", filePath: "cors.go" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
