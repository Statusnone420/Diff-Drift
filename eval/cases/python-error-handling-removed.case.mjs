// The try/except that wrapped the network fetch is removed while the fetch
// itself survives, so a transient failure now propagates as an unhandled
// exception instead of the previous fallback.
export default {
  id: "python-error-handling-removed",
  title: "try/except around a fetch is removed",
  repo: {
    project: "sync-worker",
    branch: "agent/streamline-load",
  },
  before: {
    "sync/load.py": `def load_profile(client, uid):
    try:
        return client.fetch(uid)
    except TransportError:
        return None
`,
  },
  after: {
    "sync/load.py": `def load_profile(client, uid, refresh=False):
    return client.fetch(uid)
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Error handling removed", severity: "low", filePath: "sync/load.py" }],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
