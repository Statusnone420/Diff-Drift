# Blind-agent multi-model panel (benchmark v4)

Different models each play the **blind reviewer** over the same v4 packets. The point is not to rank
the models — it's to see whether they **agree** about Diff Drift. The models are rulers; Diff Drift is
the object being measured. A case every model misses is a real Diff Drift clarity gap worth fixing; a
case only one model misses is ruler noise.

Current columns: `../answers/` is **Claude Opus 4.8** (the canonical v4 record); `sonnet-4-6/` and
`haiku-4-5/` are added panel models. `gpt-5-5/` and `gemini/` are open slots — drop answers in and
they appear automatically.

## Add a model (e.g. run it in another coding agent)

This is vendor-neutral: answers are plain JSON, so any agent or CLI can produce a column.

1. **Generate the packets** (from the repo root):
   ```bash
   npm install
   npm run eval:packets        # writes .eval/packets/<caseId>/ for all 20 cases
   ```
2. **For each case**, give your model **only** these four files — never the case definition under
   `eval/cases/`, never an oracle:
   - `.eval/packets/<caseId>/prompt.md`  (the review contract + required JSON shape)
   - `.eval/packets/<caseId>/diff-drift-report.md`
   - `.eval/packets/<caseId>/raw-git-diff.patch`
   - `.eval/packets/<caseId>/metadata.json`
   Keeping the model blind to the oracle is what makes the score meaningful.
3. **Save one answer per case** to `eval/benchmarks/v4/panel/<your-model>/<caseId>.json`, matching
   `answer-template.json`. Use a stable evaluator id (e.g. `gpt-5.5-v4`), `kind: "model"`,
   `external: false`.
4. **Score the panel:**
   ```bash
   npm run eval:panel
   ```
   It auto-discovers your folder, prints the matrix, and regenerates
   `panel-scorecard.{json,md,html}`. Re-capture the image with
   `npm run scorecard:capture -- eval/benchmarks/v4/panel/panel-scorecard.html docs/assets/diff-drift-blind-agent-scorecard.png`.

## Honesty constraints

- **Blind always:** the four packet files are the entire input. No `eval/cases/`, no oracle, no peeking
  at other models' answers.
- **An all-model panel stays `external validation pending`** — clearing that needs a *human* reviewer
  outside the project, not more models.
- **Per-model + spread, never a pooled mean.** Pooling a weak ruler with a frontier one moves the
  headline for the wrong reason. The scorecard reports each model and the range.
- The scorer (`eval/lib/score.mjs`) and the packet prompt are frozen for v4; do not tune them to lift a
  number.
