// Recreates the demo session repo at demo/payments-api: a git repo whose HEAD is
// the "before" code and whose working tree holds an AI agent's risky "after" edits.
// `analyze_session` (with no repo path) analyzes this by default. Run: npm run seed:demo
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { platform } from "node:os";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(root, "demo", "payments-api");

const before = {
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
};

const after = {
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
  // session.ts: formatting-only change (reindented + blank line) → "Formatting only".
  "routes/session.ts": `function handleSession(req: Request, res: Response) {
    return res.json({ ok: true });
}


export default router;
`,
};

const git = (...args) => execFileSync("git", ["-C", repo, ...args], { stdio: "pipe" });
const write = (set) =>
  Object.entries(set).forEach(([rel, content]) => {
    const p = join(repo, rel);
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, content);
  });

// git's object files are read-only; on Windows `rmdir /s /q` clears them, where
// fs.rmSync would hit EPERM.
function rmrf(p) {
  try {
    if (platform() === "win32") {
      execFileSync("cmd", ["/c", "rmdir", "/s", "/q", p], { stdio: "ignore" });
    } else {
      rmSync(p, { recursive: true, force: true });
    }
  } catch {
    /* nothing to remove */
  }
}

rmrf(repo);
mkdirSync(repo, { recursive: true });
write(before);
git("init", "-q");
git("config", "user.email", "demo@drift.local");
git("config", "user.name", "Drift Demo");
git("add", "-A");
git("commit", "-q", "-m", "baseline payments-api session");
git("branch", "-M", "agent/refactor-token-validation");
write(after); // uncommitted "agent" edits in the working tree

console.log(`Seeded demo repo at ${repo}`);
console.log(`  branch: ${git("rev-parse", "--abbrev-ref", "HEAD").toString().trim()}`);
console.log(`  changed: ${git("status", "--porcelain").toString().trim().split("\n").length} files`);
