// Go cookies-weakened: session-cookie protections are stripped on http.Cookie
// literals. The access cookie loses HttpOnly (scripts can now read it), the
// refresh cookie loses Secure (it may travel over plain HTTP), and the CSRF
// cookie's SameSite is downgraded from Strict to None (CSRF exposure returns).
// Three independent http.Cookie declarations, one weakening each.
export default {
  id: "go-cookies-weakened",
  title: "Go session cookies lose their protections",
  repo: {
    project: "session-go",
    branch: "agent/simplify-cookies",
  },
  before: {
    "cookies.go": `package session

var accessCookie = http.Cookie{Name: "access", HttpOnly: true}

var refreshCookie = http.Cookie{Name: "refresh", Secure: true}

var csrfCookie = http.Cookie{Name: "csrf", SameSite: http.SameSiteStrictMode}
`,
  },
  after: {
    "cookies.go": `package session

var accessCookie = http.Cookie{Name: "access"}

var refreshCookie = http.Cookie{Name: "refresh"}

var csrfCookie = http.Cookie{Name: "csrf", SameSite: http.SameSiteNoneMode}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 3,
    requiredFlags: [
      { type: "Weakened cookie flags", severity: "high", filePath: "cookies.go" },
      { type: "Weakened cookie flags", severity: "high", filePath: "cookies.go" },
      { type: "Weakened cookie flags", severity: "high", filePath: "cookies.go" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
