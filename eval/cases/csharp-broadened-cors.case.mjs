// A CORS policy is opened from a named allowlist to any origin, so credentialed
// cross-origin requests from any site are now accepted.
export default {
  id: "csharp-broadened-cors",
  title: "C# CORS policy opened to any origin",
  repo: {
    project: "public-api",
    branch: "agent/fix-cors",
  },
  before: {
    "src/CorsSetup.cs": `public class CorsSetup {
    public void Configure(CorsPolicyBuilder builder) {
        builder.WithOrigins("https://app.example.com");
    }
}
`,
  },
  after: {
    "src/CorsSetup.cs": `public class CorsSetup {
    public void Configure(CorsPolicyBuilder builder) {
        builder.AllowAnyOrigin();
    }
}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Broadened CORS", severity: "high", filePath: "src/CorsSetup.cs" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
