// An agent removes the try/catch that wrapped a remote push while "cleaning up"
// the sync routine. The call survives but its failures are now unhandled.
export default {
  id: "kotlin-error-handling-removed",
  title: "Kotlin sync drops try/catch around remote push",
  repo: {
    project: "sync-svc",
    branch: "agent/cleanup-sync",
  },
  before: {
    "src/main/kotlin/sync/Syncer.kt": `package sync

class Syncer {
    fun sync() {
        try {
            pushChanges()
        } catch (e: Exception) {
            logger.warn("push failed", e)
        }
    }
}
`,
  },
  after: {
    "src/main/kotlin/sync/Syncer.kt": `package sync

class Syncer {
    fun sync() {
        pushChanges()
    }
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Error handling removed", severity: "low", filePath: "src/main/kotlin/sync/Syncer.kt" },
    ],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
