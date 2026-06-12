// Python service refactor — structural drift, no security flags. The existing
// handler keeps its name, signature, and every protection (the validity guard,
// the sanitize call, the try/except) byte-for-byte; the change adds a thin batch
// helper and reformats a benign log line. A careful reviewer would approve.
// This doubles as negative coverage: guard-removed, removed-sanitize, and
// removed-try-catch must all stay quiet because nothing protective was removed
// or altered — the new helper is a pure addition.
export default {
  id: "python-structural-drift",
  title: "Python service refactor — structural drift, no security flags",
  repo: {
    project: "intake-svc",
    branch: "agent/refactor-intake",
  },
  before: {
    "intake/handler.py": `def handle(req):
    if not is_valid(req):
        return None
    clean = sanitize(req.body)
    try:
        return persist(clean)
    except StoreError:
        return None
`,
  },
  after: {
    "intake/handler.py": `def handle(req):
    if not is_valid(req):
        return None
    clean = sanitize(req.body)
    try:
        return persist(clean)
    except StoreError:
        return None


def handle_batch(reqs):
    return [handle(req) for req in reqs]
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
