import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";

export function createCaseRepo(caseDef) {
  const tempRoot = mkEvalTemp(caseDef.id);
  const project = caseDef.repo?.project ?? caseDef.id;
  const branch = caseDef.repo?.branch ?? "agent/eval-drift";
  const repoPath = join(tempRoot, project);
  const stateHome = join(tempRoot, "state-home");

  mkdirSync(repoPath, { recursive: true });
  mkdirSync(stateHome, { recursive: true });
  writeFiles(repoPath, caseDef.before);

  git(repoPath, "init", "-q");
  git(repoPath, "config", "user.email", "eval@diff-drift.local");
  git(repoPath, "config", "user.name", "Diff Drift Eval");
  git(repoPath, "add", "-A");
  git(repoPath, "commit", "--allow-empty", "-q", "-m", "baseline");
  git(repoPath, "branch", "-M", branch);

  applyAfter(repoPath, caseDef.before, caseDef.after);

  return {
    tempRoot,
    repoPath,
    stateHome,
    cleanup() {
      removeEvalTemp(tempRoot);
    },
  };
}

export function gitDiff(repoPath) {
  // Shows untracked files in the packet diff without committing anything.
  git(repoPath, "add", "-N", ".");
  return git(repoPath, "diff", "--no-ext-diff", "--src-prefix=a/", "--dst-prefix=b/", "--", ".");
}

export function git(repoPath, ...args) {
  return execFileSync("git", ["-C", repoPath, ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function applyAfter(repoPath, before, after) {
  for (const rel of Object.keys(before)) {
    if (!(rel in after)) {
      const abs = safeRepoPath(repoPath, rel);
      if (existsSync(abs)) {
        unlinkSync(abs);
      }
    }
  }
  writeFiles(repoPath, after);
}

function writeFiles(repoPath, files) {
  for (const [rel, content] of Object.entries(files)) {
    const abs = safeRepoPath(repoPath, rel);
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, content.replace(/\r\n/g, "\n"));
  }
}

function safeRepoPath(repoPath, rel) {
  if (rel.includes("..") || rel.startsWith("/") || /^[A-Za-z]:/.test(rel)) {
    throw new Error(`Unsafe fixture path: ${rel}`);
  }
  const abs = resolve(repoPath, rel);
  const root = resolve(repoPath);
  if (abs !== root && !abs.startsWith(`${root}\\`) && !abs.startsWith(`${root}/`)) {
    throw new Error(`Fixture path escapes repo: ${rel}`);
  }
  return abs;
}

function mkEvalTemp(id) {
  const root = join(tmpdir(), `diff-drift-eval-${id}-${process.pid}-${Date.now()}`);
  removeEvalTemp(root);
  mkdirSync(root, { recursive: true });
  return root;
}

function removeEvalTemp(path) {
  const abs = resolve(path);
  const tmp = resolve(tmpdir());
  if (abs !== tmp && !abs.startsWith(`${tmp}\\`) && !abs.startsWith(`${tmp}/`)) {
    throw new Error(`Refusing to remove non-temp eval path: ${path}`);
  }
  if (!existsSync(abs)) {
    return;
  }
  clearReadonly(abs);
  rmSync(abs, { recursive: true, force: true });
}

function clearReadonly(path) {
  if (!existsSync(path)) {
    return;
  }
  const stat = statSync(path);
  try {
    chmodSync(path, stat.mode | 0o600);
  } catch {
    // Best effort: git object cleanup on Windows is the only reason this exists.
  }
  if (stat.isDirectory()) {
    for (const entry of readdirSync(path)) {
      clearReadonly(join(path, entry));
    }
  }
}
