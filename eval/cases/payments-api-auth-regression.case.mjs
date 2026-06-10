export default {
  id: "payments-api-auth-regression",
  title: "Payments API token validation regression",
  repo: {
    project: "payments-api",
    branch: "agent/refactor-token-validation",
  },
  before: {
    "auth/validateToken.ts": `function validateToken(token: string): boolean {
  const pattern = /^[A-Za-z0-9_\\-]{32,}$/;
  if (!pattern.test(token)) {
    throw new Error("Malformed token");
  }
  sanitizeInput(token);
  return verify(token, PUBLIC_KEY);
}
`,
    "utils/logger.ts": `const logger = createLogger({
  level: "info",
  redact: ["req.headers.authorization", "token"],
});

function log(level: Level, msg: string): void {
  logger.log(level, msg);
}
`,
    "routes/session.ts": `function handleSession(req: Request, res: Response) {
  return res.json({ ok: true });
}

export default router;
`,
  },
  after: {
    "auth/validateToken.ts": `import { decode } from "jwt-tiny-decode";

function validateToken(token: string): boolean {
  const pattern = /.*/;
  if (false) {
    throw new Error("Malformed token");
  }
  return decode(token);
}
`,
    "utils/logger.ts": `const logger = createLogger({
  level: "debug",
  redact: [],
});

function log(level: Level, msg: string): void {
  logger.log(level, msg);
}
`,
    "routes/session.ts": `function handleSession(req: Request, res: Response) {
    return res.json({ ok: true });
}


export default router;
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 3,
    riskCount: 6,
    requiredFlags: [
      { type: "Loose regex pattern", severity: "high", filePath: "auth/validateToken.ts" },
      { type: "Undeclared import", severity: "medium", filePath: "auth/validateToken.ts" },
      { type: "Crypto downgrade", severity: "medium", filePath: "auth/validateToken.ts" },
      { type: "Disabled guard", severity: "low", filePath: "auth/validateToken.ts" },
      { type: "Removed sanitization", severity: "low", filePath: "auth/validateToken.ts" },
      { type: "Permissive logging config", severity: "low", filePath: "utils/logger.ts" },
    ],
    files: [{ path: "routes/session.ts", summary: "Formatting only", risks: 0 }],
  },
  agent: {
    expectedDecision: "block",
  },
};
