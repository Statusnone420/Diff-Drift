// Go guard-removed: a call that ran only inside an `if err != nil { … }` check
// before the change now runs for every record. The before code quarantines a
// record only when the inspection returns an error; the agent's "simplification" drops the
// `if err != nil` guard, so `quarantine` runs unconditionally — quarantining
// every record, valid or not. Only a diff-native engine sees this: the after
// state alone reads like ordinary code. This is the Go error-check idiom the
// guard-removed rule exists to catch (Go has no try/catch — its error handling
// IS the `if err != nil` guard, so guard-removed covers it).
export default {
  id: "go-guard-removed",
  title: "Go ingest loses its error-check guard",
  repo: {
    project: "ingest-go",
    branch: "agent/simplify-ingest",
  },
  before: {
    "ingest.go": `package ingest

func Run(batch []Record) {
    for _, rec := range batch {
        if err := inspect(rec); err != nil {
            quarantine(rec)
        }
    }
}
`,
  },
  after: {
    "ingest.go": `package ingest

func Run(batch []Record) {
    for _, rec := range batch {
        quarantine(rec)
    }
}
`,
  },
  oracle: {
    expectedExitCode: 2,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Guard removed", severity: "medium", filePath: "ingest.go" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
