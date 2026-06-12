// An agent collapses a do/catch around a failing persist into a `try?`, which
// silently swallows the error — the failure is now unhandled.
export default {
  id: "swift-error-handling-removed",
  title: "Swift do/catch dropped for silent try?",
  repo: {
    project: "sync-ios",
    branch: "agent/quiet-persist",
  },
  before: {
    "Sources/Sync/Store.swift": `import Foundation

func saveAll(records: [Record]) {
    for record in records {
        do {
            try persist(record)
        } catch {
            log(error)
        }
    }
}
`,
  },
  after: {
    "Sources/Sync/Store.swift": `import Foundation

func saveAll(records: [Record]) {
    for record in records {
        let saved = try? persist(record)
    }
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Error handling removed", severity: "low", filePath: "Sources/Sync/Store.swift" },
    ],
  },
  agent: {
    expectedDecision: "approve",
  },
};
