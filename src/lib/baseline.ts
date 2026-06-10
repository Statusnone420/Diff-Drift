import type { Session } from "../types";

/**
 * Human-readable phrase for what the drift is measured against. Written to
 * read naturally after "since": "no changes since the last commit (HEAD)".
 * Keeps every clean/empty state honest about the SELECTED baseline instead of
 * assuming HEAD.
 */
export function baselinePhrase(session: Session): string {
  if (session.baselineSpec !== "head" && session.baselineLabel === "HEAD") {
    return "the last commit (HEAD)";
  }

  switch (session.baselineSpec) {
    case "head":
      return "the last commit (HEAD)";
    case "trust-point":
      return session.trustPoint
        ? `your last review (trust point ${session.trustPoint})`
        : "your last review (trust point)";
    case "merge-base":
      return "the branch start (merge-base)";
    default:
      return `"${session.baselineSpec}"`;
  }
}
