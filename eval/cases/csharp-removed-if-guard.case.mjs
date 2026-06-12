// A feature-flag check is neutralized to a constant `false`, so the guarded
// branch never runs again. The condition still reads like a check at a glance.
export default {
  id: "csharp-removed-if-guard",
  title: "C# guard condition replaced with constant false",
  repo: {
    project: "admin-tools",
    branch: "agent/toggle-cleanup",
  },
  before: {
    "src/AccessControl.cs": `public class AccessControl {
    public void Apply(User user) {
        if (IsAdmin(user)) {
            GrantElevatedAccess(user);
        }
    }
}
`,
  },
  after: {
    "src/AccessControl.cs": `public class AccessControl {
    public void Apply(User user) {
        if (false) {
            GrantElevatedAccess(user);
        }
    }
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Disabled guard", severity: "low", filePath: "src/AccessControl.cs" }],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
