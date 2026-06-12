// The try/catch that wrapped a network call is removed while the call survives,
// so a failure that used to be caught and logged now propagates unhandled.
export default {
  id: "csharp-error-handling-removed",
  title: "C# try/catch around a network call removed",
  repo: {
    project: "sync-worker",
    branch: "agent/cleanup-error-handling",
  },
  before: {
    "src/Syncer.cs": `public class Syncer {
    public void Run() {
        try {
            Connect();
        } catch (Exception e) {
            Log(e);
        }
    }
}
`,
  },
  after: {
    "src/Syncer.cs": `public class Syncer {
    public void Run() {
        Connect();
    }
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Error handling removed", severity: "low", filePath: "src/Syncer.cs" }],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
