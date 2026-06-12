// The session cookie loses its secure flag, so it may now be sent over plain
// HTTP. httponly stays set, so the secure removal is the single planted change
// (the httponly rule, which runs first, finds nothing to flag).
export default {
  id: "python-cookie-secure-removed",
  title: "Session cookie loses secure",
  repo: {
    project: "web-app",
    branch: "agent/cookie-tweak",
  },
  before: {
    "web/session.py": `def set_session(resp, sid):
    resp.set_cookie("sid", sid, httponly=True, secure=True, samesite="Lax")
    return resp
`,
  },
  after: {
    "web/session.py": `def set_session(resp, sid, remember=False):
    resp.set_cookie("sid", sid, httponly=True, samesite="Lax")
    return resp
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Weakened cookie flags", severity: "high", filePath: "web/session.py" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
