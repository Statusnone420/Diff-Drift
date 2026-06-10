import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { projectRoot } from "./cases.mjs";

let defaultBuildDone = false;

export function runDiffDrift(repoPath, stateHome, format = "json") {
  mkdirSync(stateHome, { recursive: true });
  const args = ["check", repoPath, format === "md" ? "--md" : "--json"];
  const command = diffDriftCommand(args);
  prepareDiffDriftCommand(command);
  const result = spawnSync(command.bin, command.args, {
    cwd: projectRoot,
    encoding: "utf8",
    env: diffDriftRuntimeEnv(stateHome),
  });

  if (result.error) {
    throw result.error;
  }

  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  const exitCode = result.status ?? 1;
  if (exitCode === 64) {
    throw new Error(`diff-drift check failed: ${stderr || stdout}`);
  }

  if (format === "md") {
    return { exitCode, stdout, stderr, command };
  }

  try {
    return { exitCode, data: JSON.parse(stdout), stdout, stderr, command };
  } catch (error) {
    throw new Error(`diff-drift emitted invalid JSON: ${error.message}\n${stdout}\n${stderr}`);
  }
}

export function diffDriftCommand(checkArgs) {
  const bin = process.env.DIFF_DRIFT_EVAL_BIN;
  if (bin) {
    return { bin, args: checkArgs };
  }
  return {
    bin: debugBinaryPath(),
    args: checkArgs,
    build: {
      bin: "cargo",
      args: ["build", "--quiet", "--manifest-path", "src-tauri/Cargo.toml", "--bin", "diff-drift"],
    },
  };
}

// Build the debug binary if this checkout hasn't yet (no-op with
// DIFF_DRIFT_EVAL_BIN set). Callers that spawn the binary directly —
// like scripts/fp-replay.mjs — must run this once first, or a clean
// checkout fails with a missing-binary error.
export function ensureDiffDriftBuilt() {
  prepareDiffDriftCommand(diffDriftCommand([]));
}

export function diffDriftRuntimeEnv(stateHome) {
  return {
    ...process.env,
    APPDATA: stateHome,
    HOME: stateHome,
    XDG_CONFIG_HOME: stateHome,
  };
}

function prepareDiffDriftCommand(command) {
  if (!command.build || defaultBuildDone) {
    return;
  }

  const result = spawnSync(command.build.bin, command.build.args, {
    cwd: projectRoot,
    encoding: "utf8",
  });

  if (result.error) {
    throw result.error;
  }

  const exitCode = result.status ?? 1;
  if (exitCode !== 0) {
    throw new Error(`cargo build failed: ${result.stderr || result.stdout}`);
  }

  defaultBuildDone = true;
}

function debugBinaryPath() {
  const name = process.platform === "win32" ? "diff-drift.exe" : "diff-drift";
  return join(projectRoot, "src-tauri", "target", "debug", name);
}
