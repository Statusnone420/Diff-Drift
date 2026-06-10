#!/usr/bin/env node
// FP-replay: measure Diff Drift's flag noise on YOUR repos instead of
// synthetic fixtures. Point fp-replay.config.json at local repos and
// baselines you consider benign (a merged branch, a routine commit range) —
// every flag reported there is a false positive for your codebase, which is
// the number that predicts real triage burden.
//
// Local-only by design: nothing is bundled, nothing is uploaded, and the
// state home is isolated so your dismissals don't hide flags (this measures
// the tool, not your triage).
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { projectRoot } from "../eval/lib/cases.mjs";
import {
  diffDriftCommand,
  diffDriftRuntimeEnv,
  ensureDiffDriftBuilt,
} from "../eval/lib/cli.mjs";

const CHECK_OUTPUT_MAX_BUFFER = 64 * 1024 * 1024;

const configPath = resolve(projectRoot, process.argv[2] ?? "fp-replay.config.json");
if (!existsSync(configPath)) {
  console.error(`No config at ${configPath}.`);
  console.error("Copy fp-replay.config.example.json to fp-replay.config.json and list your repos.");
  process.exit(64);
}

const config = JSON.parse(readFileSync(configPath, "utf8"));
const targets = Array.isArray(config.targets) ? config.targets : [];
if (targets.length === 0) {
  console.error('Config has no "targets". Each target needs { "repoPath": ..., "baseline": ... }.');
  process.exit(64);
}

ensureDiffDriftBuilt(); // clean checkouts get the CLI built, not an ENOENT

const results = [];
for (const target of targets) {
  results.push(replayTarget(target));
}

const report = buildReport(results);
const outDir = join(projectRoot, ".eval", "results", "fp-replay");
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, "latest.json"), `${JSON.stringify(report, null, 2)}\n`);
writeFileSync(join(outDir, "latest.md"), renderMarkdown(report));
console.log(renderMarkdown(report));
console.log(`Written: ${join(outDir, "latest.md")}`);
process.exit(report.errors.length > 0 ? 1 : 0);

function replayTarget(target) {
  const repoPath = resolve(String(target.repoPath ?? ""));
  const label = target.label ?? repoPath;
  const baseline = target.baseline ?? "head";
  if (!target.repoPath || !existsSync(repoPath)) {
    return { label, repoPath, baseline, error: `repo path not found: ${repoPath}` };
  }

  // Isolated state home: dismissals must not hide flags in this measurement.
  const stateHome = join(tmpdir(), `diff-drift-fp-replay-${process.pid}-${results.length}`);
  rmSync(stateHome, { recursive: true, force: true });
  mkdirSync(stateHome, { recursive: true });
  try {
    const command = diffDriftCommand(["check", repoPath, "--json", "--baseline", String(baseline)]);
    const run = spawnSync(command.bin, command.args, {
      cwd: projectRoot,
      encoding: "utf8",
      env: diffDriftRuntimeEnv(stateHome),
      maxBuffer: CHECK_OUTPUT_MAX_BUFFER,
    });
    if (run.error) {
      return { label, repoPath, baseline, error: run.error.message };
    }
    if ((run.status ?? 1) === 64) {
      return { label, repoPath, baseline, error: (run.stderr || run.stdout).trim() };
    }
    const data = JSON.parse(run.stdout);
    const activeFlags = (data.flags ?? []).filter((flag) => !flag.dismissed);
    const byRule = {};
    for (const flag of activeFlags) {
      byRule[flag.type] = (byRule[flag.type] ?? 0) + 1;
    }
    return {
      label,
      repoPath,
      baseline,
      exitCode: run.status ?? 0,
      changedFiles: data.session?.changedFiles ?? 0,
      analyzedFiles: (data.files ?? []).length,
      activeFlags: activeFlags.length,
      flagsPerChangedFile:
        (data.session?.changedFiles ?? 0) === 0
          ? 0
          : activeFlags.length / data.session.changedFiles,
      byRule,
    };
  } catch (error) {
    return { label, repoPath, baseline, error: error.message };
  } finally {
    rmSync(stateHome, { recursive: true, force: true });
  }
}

function buildReport(scored) {
  const ok = scored.filter((entry) => !entry.error);
  const perRuleTotals = {};
  for (const entry of ok) {
    for (const [rule, count] of Object.entries(entry.byRule)) {
      perRuleTotals[rule] = (perRuleTotals[rule] ?? 0) + count;
    }
  }
  return {
    generatedAt: new Date().toISOString(),
    note: "Flags on changes you consider benign are false positives for your codebase. Dismissals are intentionally ignored in this measurement.",
    targets: scored,
    perRuleTotals,
    totals: {
      targets: scored.length,
      changedFiles: ok.reduce((sum, entry) => sum + entry.changedFiles, 0),
      activeFlags: ok.reduce((sum, entry) => sum + entry.activeFlags, 0),
    },
    errors: scored.filter((entry) => entry.error).map((entry) => `${entry.label}: ${entry.error}`),
  };
}

function renderMarkdown(report) {
  const lines = [
    "# Diff Drift FP-replay",
    "",
    `Generated: ${report.generatedAt}`,
    "",
    `> ${report.note}`,
    "",
    "| Target | Baseline | Changed files | Active flags | Flags / changed file |",
    "| --- | --- | ---: | ---: | ---: |",
  ];
  for (const entry of report.targets) {
    if (entry.error) {
      lines.push(`| ${entry.label} | ${entry.baseline} | — | — | error: ${entry.error.split("\n")[0]} |`);
    } else {
      lines.push(
        `| ${entry.label} | ${entry.baseline} | ${entry.changedFiles} | ${entry.activeFlags} | ${entry.flagsPerChangedFile.toFixed(2)} |`,
      );
    }
  }
  const rules = Object.entries(report.perRuleTotals).sort(([, a], [, b]) => b - a);
  lines.push("", "## Flags by rule", "");
  if (rules.length === 0) {
    lines.push("No active flags across the configured targets.");
  } else {
    lines.push("| Rule | Flags |", "| --- | ---: |");
    for (const [rule, count] of rules) {
      lines.push(`| ${rule} | ${count} |`);
    }
  }
  if (report.errors.length > 0) {
    lines.push("", "## Errors", "");
    for (const error of report.errors) {
      lines.push(`- ${error}`);
    }
  }
  lines.push("");
  return lines.join("\n");
}
