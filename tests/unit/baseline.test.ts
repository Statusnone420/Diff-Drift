import { describe, expect, it } from "vitest";
import { baselinePhrase } from "../../src/lib/baseline";
import { makeSession } from "./helpers";

// Every clean/empty state interpolates this phrase — it is the single point
// keeping UI copy honest about the SELECTED baseline instead of assuming HEAD.
describe("baselinePhrase", () => {
  it("names the last commit for the head baseline", () => {
    expect(baselinePhrase(makeSession())).toBe("the last commit (HEAD)");
  });

  it("names the pinned trust point for the trust-point baseline", () => {
    const s = makeSession({ baselineSpec: "trust-point", trustPoint: "ab12cd3" });
    expect(baselinePhrase(s)).toBe("your last review (trust point ab12cd3)");
  });

  it("stays readable when trust-point is selected but nothing is pinned", () => {
    const s = makeSession({ baselineSpec: "trust-point" });
    expect(baselinePhrase(s)).toBe("your last review (trust point)");
  });

  it("names the branch start for the merge-base baseline", () => {
    const s = makeSession({ baselineSpec: "merge-base" });
    expect(baselinePhrase(s)).toBe("the branch start (merge-base)");
  });

  it("quotes a custom ref verbatim", () => {
    const s = makeSession({ baselineSpec: "release/v1.2" });
    expect(baselinePhrase(s)).toBe('"release/v1.2"');
  });
});
