export default {
  id: "swift-structural-drift",
  title: "Swift service refactor — structural drift, no security flags",
  repo: {
    project: "orders-ios",
    branch: "agent/refactor-service",
  },
  before: {
    "Sources/Orders/Service.swift": `import Foundation

func process(input: String) -> Bool {
    let body = read(input)
    if isVerified(body) {
        handle(body)
    }
    return true
}
`,
  },
  after: {
    "Sources/Orders/Service.swift": `import Foundation

// Renamed, reordered, and reformatted — the verification guard is preserved.
func process(rawInput: String) -> Bool {
    let parsed = read(rawInput)

    if isVerified(parsed) {
        handle(parsed)
    }

    return true
}
`,
  },
  oracle: {
    // Stretch languages get STRUCTURAL drift only here — the guard is preserved
    // through a benign rename/reorder/reformat, so no flag is raised.
    expectedExitCode: 0,
    changedFiles: 1,
    riskCount: 0,
    requiredFlags: [],
    forbiddenFlags: [{ severity: "high" }, { severity: "medium" }, { severity: "low" }],
  },
  agent: {
    expectedDecision: "approve",
  },
};
