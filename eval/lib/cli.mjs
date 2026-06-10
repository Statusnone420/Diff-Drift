import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { projectRoot } from "./cases.mjs";

export function runDiffDrift(repoPath, stateHome, format = "json") {
  mkdirSync(stateHome, { recursive: true });
  const args = ["check", repoPath, format === "md" ? "--md" : "--json"];
  const command = diffDriftCommand(args);
  const result = spawnSync(command.bin, command.args, {
    cwd: projectRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      APPDATA: stateHome,
      HOME: stateHome,
      XDG_CONFIG_HOME: stateHome,
    },
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

function diffDriftCommand(checkArgs) {
  const bin = configuredBinary();
  if (bin) {
    return { bin, args: checkArgs };
  }
  return {
    bin: "cargo",
    args: ["run", "--quiet", "--manifest-path", "src-tauri/Cargo.toml", "--", ...checkArgs],
  };
}

function configuredBinary() {
  if (process.env.DIFF_DRIFT_EVAL_BIN) {
    return process.env.DIFF_DRIFT_EVAL_BIN;
  }
  for (const candidate of debugBinaryCandidates()) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

function debugBinaryCandidates() {
  const name = process.platform === "win32" ? "diff-drift.exe" : "diff-drift";
  return [
    join(projectRoot, "src-tauri", "target", "debug", name),
    join(projectRoot, "src-tauri", "target", "release", name),
  ];
}
