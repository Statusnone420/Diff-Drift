import { existsSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

// Resolve answer-file arguments for scoring: explicit files pass through,
// a directory expands to its .json files (sorted), and no args falls back
// to the default answers directory. Lets a fresh clone rescore a committed
// benchmark snapshot: `npm run eval:score-agent -- eval/benchmarks/v4/answers`.
export function collectAnswerFiles(args, defaultDir, baseDir = process.cwd()) {
  if (args.length > 0) {
    return args.flatMap((arg) => {
      const abs = resolve(baseDir, arg);
      if (existsSync(abs) && statSync(abs).isDirectory()) {
        return jsonFilesIn(abs);
      }
      return [abs];
    });
  }
  return jsonFilesIn(defaultDir);
}

export function answerFileLabel(file, baseDir = process.cwd()) {
  return (relative(baseDir, file) || ".").replaceAll("\\", "/");
}

function jsonFilesIn(dir) {
  if (!existsSync(dir)) {
    return [];
  }
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".json"))
    .map((entry) => join(dir, entry.name))
    .sort();
}
