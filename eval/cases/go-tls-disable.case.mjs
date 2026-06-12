// Go tls-disable: certificate validation is switched off by adding
// InsecureSkipVerify: true to the tls.Config. The client will now accept any
// certificate, including a forged or self-signed one — a MITM opening.
export default {
  id: "go-tls-disable",
  title: "Go HTTP client disables certificate validation",
  repo: {
    project: "webhook-go",
    branch: "agent/fix-staging-tls",
  },
  before: {
    "client.go": `package webhook

func tlsConfig() *tls.Config {
    cfg := &tls.Config{MinVersion: tls.VersionTLS12}
    return cfg
}
`,
  },
  after: {
    "client.go": `package webhook

func tlsConfig() *tls.Config {
    cfg := &tls.Config{MinVersion: tls.VersionTLS12, InsecureSkipVerify: true}
    return cfg
}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Disabled TLS verification", severity: "high", filePath: "client.go" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
