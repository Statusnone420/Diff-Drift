export default {
  id: "rust-structural-drift",
  title: "Rust handler refactor — structural drift, no security flags",
  repo: {
    project: "edge-proxy",
    branch: "agent/refactor-handler",
  },
  before: {
    "src/handler.rs": `pub fn handle(req: &Request) -> Response {
    let body = read_body(req);
    if body.is_empty() {
        return Response::empty();
    }
    process(body)
}
`,
  },
  after: {
    "src/handler.rs": `pub fn handle(req: &Request) -> Response {
    let body = read_body(req);
    process(body)
}

pub fn shutdown() {
    cleanup();
}
`,
  },
  oracle: {
    // New languages get STRUCTURAL drift only — no JS-specific security rules.
    // The guard was dropped and a function added, but no flag is raised.
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
