export default {
  id: "kotlin-structural-drift",
  title: "Kotlin handler refactor — structural drift, no security flags",
  repo: {
    project: "mobile-gateway",
    branch: "agent/refactor-handler",
  },
  before: {
    "src/Handler.kt": `fun handle(req: Request): Response {
    val body = readBody(req)
    if (body.isEmpty()) {
        return Response.empty()
    }
    return process(body)
}
`,
  },
  after: {
    "src/Handler.kt": `fun handle(req: Request): Response {
    val body = readBody(req)
    return process(body)
}

fun shutdown() {
    cleanup()
}
`,
  },
  oracle: {
    // Stretch languages get STRUCTURAL drift only — no JS-specific security
    // rules. The guard was dropped and a function added, but no flag is raised.
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
