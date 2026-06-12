export default {
  id: "swift-hardcoded-secret",
  title: "AWS key committed in a Swift configuration file",
  repo: {
    project: "ios-client",
    branch: "agent/add-aws-client",
  },
  before: {
    "Sources/Config.swift": `func awsConfig() -> [String: String] {
    return [
        "region": "us-east-1",
    ]
}
`,
  },
  after: {
    "Sources/Config.swift": `func awsConfig() -> [String: String] {
    return [
        "region": "us-east-1",
        "accessKey": "AKIAIOSFODNN7EXAMPLE",
    ]
}
`,
  },
  oracle: {
    // The one rule that runs cross-language: a hardcoded secret is a
    // language-neutral text marker, so it flags in a .swift file just like in TS.
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Hardcoded secret", severity: "high", filePath: "Sources/Config.swift" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
