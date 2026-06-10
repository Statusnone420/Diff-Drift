import { readdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const casesDir = join(projectRoot, "eval", "cases");

export function parseCaseArgs(args) {
  const ids = [];
  let keep = false;
  let json = false;

  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--case") {
      const id = args[i + 1];
      if (!id) {
        throw new Error("--case needs an id");
      }
      ids.push(id);
      i += 1;
    } else if (arg.startsWith("--case=")) {
      ids.push(arg.slice("--case=".length));
    } else if (arg === "--keep") {
      keep = true;
    } else if (arg === "--json") {
      json = true;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return { ids, keep, json };
}

export async function loadCases(ids = []) {
  const wanted = new Set(ids);
  const entries = await readdir(casesDir, { withFileTypes: true });
  const files = entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".case.mjs"))
    .map((entry) => join(casesDir, entry.name))
    .sort();

  const cases = [];
  for (const file of files) {
    const mod = await import(pathToFileURL(file).href);
    const caseDef = mod.default;
    validateCase(caseDef, file);
    if (wanted.size === 0 || wanted.has(caseDef.id)) {
      cases.push(caseDef);
    }
  }

  for (const id of wanted) {
    if (!cases.some((caseDef) => caseDef.id === id)) {
      throw new Error(`No eval case found for "${id}"`);
    }
  }

  return cases;
}

function validateCase(caseDef, file) {
  if (!caseDef || typeof caseDef !== "object") {
    throw new Error(`${file} must export a case object`);
  }
  for (const key of ["id", "title", "before", "after", "oracle"]) {
    if (!(key in caseDef)) {
      throw new Error(`${file} is missing "${key}"`);
    }
  }
  if (!/^[a-z0-9][a-z0-9-]*$/.test(caseDef.id)) {
    throw new Error(`${file} has an invalid id: ${caseDef.id}`);
  }
  if (!isFileMap(caseDef.before) || !isFileMap(caseDef.after)) {
    throw new Error(`${file} before/after values must be file maps`);
  }
  if (!Number.isInteger(caseDef.oracle.expectedExitCode)) {
    throw new Error(`${file} oracle.expectedExitCode must be an integer`);
  }
}

function isFileMap(value) {
  return (
    value &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    Object.entries(value).every(
      ([name, content]) => typeof name === "string" && typeof content === "string",
    )
  );
}
