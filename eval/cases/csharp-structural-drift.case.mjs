export default {
  id: "csharp-structural-drift",
  title: "C# service refactor — structural drift, no security flags",
  repo: {
    project: "orders-api",
    branch: "agent/refactor-service",
  },
  before: {
    "src/OrderService.cs": `public class OrderService {
    public bool Process(string input) {
        var body = Read(input);
        if (string.IsNullOrEmpty(body)) {
            return false;
        }
        return Handle(body);
    }
}
`,
  },
  after: {
    "src/OrderService.cs": `public class OrderService {
    public bool Process(string input) {
        var body = Read(input);
        return Handle(body);
    }

    public void Shutdown() {
        Cleanup();
    }
}
`,
  },
  oracle: {
    // Stretch languages get STRUCTURAL drift only — no JS-specific security
    // rules. The guard was dropped and a method added, but no flag is raised.
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
