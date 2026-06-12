export default {
  id: "go-structural-drift",
  title: "Go service refactor — structural drift, no security flags",
  repo: {
    project: "ingest-worker",
    branch: "agent/refactor-ingest",
  },
  before: {
    "ingest.go": `package ingest

func Run(batch []Event) error {
    if len(batch) == 0 {
        return nil
    }
    return flush(batch)
}
`,
  },
  after: {
    "ingest.go": `package ingest

func Run(batch []Event) error {
    return flush(batch)
}

func Drain() error {
    return flush(pending())
}
`,
  },
  oracle: {
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
