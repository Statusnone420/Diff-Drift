# Eval Methodology

How Diff Drift's evaluation works, exactly what the numbers mean, and — just as important — what they do not mean. Commands and file locations live in [Development](Development.md#evaluation-harness); this page is the methodology contract.

## Two Different Measurements

| | Engine eval | Blind-agent scorecard |
| --- | --- | --- |
| Question | Does the engine produce exactly the expected flags, counts, and exit code? | Can a reviewer who sees only Diff Drift's output reach the right decision and cite the right evidence? |
| Gate | **CI blocker** (`npm run eval:engine`) | Advisory, local only |
| Scoring | Binary pass/fail per case against the oracle | 0–100 rubric per case |
| Measures | The tool | The tool's *usefulness to a reviewer* — not the tool's detection rate |

## Cases

Each case in `eval/cases/*.case.mjs` defines a before/after repo state and an oracle: expected exit code, changed-file and risk counts, required flags (type, severity, file), forbidden flags, and per-file summaries. Fixtures are built as real temporary git repos and run through the real `diff-drift check` binary — nothing is mocked.

The suite currently covers: one case per high-severity rule family (secrets, eval, child_process, TLS, CORS, cookies), the multi-flag payments-api regression, dependency/script drift, TSX/JSX/.mjs files, a test-fixture suppression case, two benign cases (formatting-only; a rename-heavy noisy refactor), and the oversized-file skip guard.

### Per-rule coverage

Rules with at least one dedicated engine case: Hardcoded secret, Dynamic code execution, Child process execution, Disabled TLS verification, Broadened CORS, Weakened cookie flags, Loose regex pattern, Crypto downgrade, Undeclared import, Disabled guard, Removed sanitization, Permissive logging config, Dependency not in lockfile, New dependency, npm script changed, Dependency version changed (the last three inside the dependency-drift case). Known gaps worth future cases: scoped-package slopsquatting, lockfile-format edge cases per package manager, deeper TSX surface (hooks, props spreading).

**Limitations of the case suite:** every fixture is synthetic and small (a handful of files). There is no real-world corpus yet, so no claim is made about recall or precision on production diffs — measuring that on your own repos is what `npm run eval:fp-replay` exists for.

## Blind-Agent Rubric

A blind packet contains the Diff Drift markdown report, the raw git diff, and a prompt. It never contains the oracle. The reviewer returns a decision (`approve | investigate | block`), a findings list, and (since benchmark v2) an optional `notes` list for benign observations and report feedback — notes are validated for shape but **ignored entirely by scoring**, no credit and no penalty. `eval/lib/score.mjs` scores the rest:

```
score = recall × 60
      + (decision accepted ? 20 : 0)
      + (top risk ranked first ? 10 : 0)
      + localization × 10
      − false positives × 5
      − (wrong decision on a benign case ? 50 : 0)
```

- **Recall** is severity-weighted: low = 1, medium = 2, high = 3. Missing a High costs three times a Low.
- **Localization**: a matched finding must name the expected file path.
- **False positives**: every reported finding that matches no expected flag costs 5 points. On benign cases (zero required flags), any finding zeroes recall — always-block, always-find strategies cannot win.
- **Decision calibration**: some cases accept more than one decision (e.g. `investigate` or `block` for a low-severity XSS-shaped change) when both are defensible review behavior. Accepted decisions are declared in the case file.

### Semantic aliases (published rubric, frozen)

Finding text is matched against the expected flag type plus these aliases (global table in `eval/lib/score.mjs`; cases may add per-flag aliases, declared in the case file):

| Flag type | Global aliases |
| --- | --- |
| Dependency not in lockfile | dependency not in lockfile, lockfile, dependency drift |
| npm script changed | npm script, install script, postinstall |
| Weakened cookie flags | weakened cookie flags, cookie flags, httponly, secure, samesite |
| Permissive logging config | permissive logging, logger redaction, redaction removed |
| Undeclared import | undeclared import, undeclared dependency, not declared |

**Frozen-rubric policy:** rubric weights, aliases, and accepted decisions are calibrated *before* answers are generated and are never tuned afterward to improve a score. If a defensible answer scores badly, that is reported as a rubric limitation — changing the rubric invalidates every previously published number and is treated as starting a new benchmark version (see below).

## Benchmark Versions

Every instrument change starts a new version; old scores are never recomputed under new rules, and new answers are always regenerated from scratch.

- **v1** (2026-06-09): original prompt, 15 synthetic cases, one blind model evaluator. **Scored 72/100** — decision accuracy 15/15 and 100% per-rule recall, but precision was 43%: the prompt didn't say where benign observations belong, so the reviewer put commentary in `findings` and the zero-oracle cases (any finding zeroes recall there) collapsed to 15–20 points. That's a prompt-contract ambiguity, not a detection failure — but 72 is the honest v1 number and stands.
- **v2** (2026-06-10): the packet prompt defines the contract explicitly — findings are actionable trust risks only, benign observations go in a new scoring-ignored `notes` field, `approve` normally means empty findings, and a size-skipped file with a clean diff is a note, not a finding. The JSX secret fixture also became realistic (non-docs-example key, actually wired into the upload call) so reviewers have nothing legitimate to caveat. All 15 answers regenerated blind under the new packets. **Scored 98/100.** Clarification (same version, no score impact): the validator now enforces the full finding shape the prompt always required — `severity`, `filePath`, `riskType`, and `evidence` are mandatory, so a title-only finding cannot collect evidence-and-location credit. Every recorded v2 answer already satisfied the shape; the rescore is unchanged.

## Evaluators and Honesty Constraints

Every answer file records who produced it (`evaluator: { id, kind: "model" | "human", note }`). The scorecard lists all evaluators and renders a standing banner — **"independent external validation pending"** — until at least two evaluators exist and at least one is a human outside the project. That banner is computed by the harness, not hand-written, so it cannot be quietly dropped.

What the current scorecard therefore is: an internal product-quality signal on a small synthetic suite. What it is not: third-party validation, a detection-rate claim, or a comparison against other tools. Treat any headline number accordingly, and check the scorecard itself for case count and evaluator list.

## Reproducing the Published Score

Every published benchmark's raw answers and scorecard are committed under `eval/benchmarks/<version>/` — the working `.eval/` directory stays gitignored, but the published evidence does not. From a fresh clone:

```bash
npm install
npm run eval:score-agent -- eval/benchmarks/v2/answers
```

That rescores the exact recorded answers through the current rubric and must print the published number (v2: 98/100). The committed `scorecard.md`/`scorecard.json` are the outputs of the original run for diffing.

## Reported Metrics

The scorecard reports, per run: overall score, decision accuracy, severity-weighted recall, localization, **precision** (matched findings ÷ all reported findings), total unmatched findings, and **per-rule recall** (matched/required per flag type across all cases that require it). Per-case rows include misses, mislocalizations, and unmatched findings verbatim.

## FP-Replay: Measuring Noise on Your Own Repos

Synthetic cases cannot tell you the triage burden on your codebase. `npm run eval:fp-replay` runs `diff-drift check` over a list of local repos and baselines you configure (`fp-replay.config.json`, see `fp-replay.config.example.json`) and aggregates active flags per rule type and per changed file into `.eval/results/fp-replay/latest.md`. Point it at a few recent merged branches you consider benign: every flag it reports is a false positive *for your code*, which is the number that predicts real triage cost. Nothing is bundled or uploaded; it only reads repos you name.

## Pending Work (tracked honestly)

- Independent human evaluators for the blind suite (clears the banner).
- A real-world labeled corpus to measure recall/precision beyond synthetic fixtures.
- The packet-vs-raw-diff A/B study — pre-registered design in [A/B Study Design](AB-Study-Design.md), no results yet.
