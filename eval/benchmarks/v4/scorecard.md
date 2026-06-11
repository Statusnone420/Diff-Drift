# Diff Drift blind-agent scorecard

Generated: 2026-06-11T01:34:25.891Z

> Advisory only: this score is not a CI gate. The CI blocker is `npm run eval:engine`; blind-agent scoring measures whether reviewers can use Diff Drift packets to reach the right evidence and decision.

Evaluators: claude-opus-4-8-v4-batch-a (model, 7 cases), claude-opus-4-8-v4-batch-b (model, 7 cases), claude-opus-4-8-v4-batch-c (model, 6 cases)

> **Independent external validation pending.** No external human evaluator has contributed answers yet. Treat the score as an internal product-quality signal, not third-party validation.

Overall score: [#########.] 94/100

- Decision accuracy: 90%
- Finding recall: 95%
- Localization: 100%
- Precision: 96% — 23 matched of 24 reported (0 near-misses, 1 false positive; both lower precision)

| Case | Score | Decision | Recall | Notes |
| --- | ---: | --- | ---: | --- |
| test-fixture-suppression | 0 | miss (approve) | 0% | decision expected approve; 1 unmatched; wrong decision on benign case |
| try-catch-removed | 80 | miss (block) | 100% | decision expected block |
| payments-api-auth-regression | 90 | ok (block) | 100% | clean |
| benign-eval-in-string | 100 | ok (approve) | 100% | clean |
| benign-formatting-only | 100 | ok (approve) | 100% | clean |
| broadened-cors | 100 | ok (block) | 100% | clean |
| child-process-execution | 100 | ok (block) | 100% | clean |
| constant-falsy-guard-evasion | 100 | ok (block) | 100% | clean |
| disabled-tls-verification | 100 | ok (block) | 100% | clean |
| dynamic-code-execution | 100 | ok (block) | 100% | clean |
| guard-removed-around-call | 100 | ok (block) | 100% | clean |
| hardcoded-secret | 100 | ok (block) | 100% | clean |
| jsx-hardcoded-secret | 100 | ok (block) | 100% | clean |
| mjs-disabled-tls | 100 | ok (block) | 100% | clean |
| noisy-benign-refactor | 100 | ok (approve) | 100% | clean |
| oversized-file-skip | 100 | ok (investigate/approve) | 100% | clean |
| package-dependency-script-drift | 100 | ok (investigate/block) | 100% | clean |
| regex-anchors-removed | 100 | ok (block) | 100% | clean |
| tsx-removed-sanitization | 100 | ok (investigate/block) | 100% | clean |
| weakened-cookie-flags | 100 | ok (block) | 100% | clean |

## Per-rule recall

Across every case that required the flag type:

| Flag type | Matched / Required | Recall |
| --- | ---: | ---: |
| Broadened CORS | 1/1 | 100% |
| Child process execution | 1/1 | 100% |
| Crypto downgrade | 1/1 | 100% |
| Dependency not in lockfile | 1/1 | 100% |
| Disabled guard | 2/2 | 100% |
| Disabled TLS verification | 2/2 | 100% |
| Dynamic code execution | 1/1 | 100% |
| Error handling removed | 1/1 | 100% |
| Guard removed | 1/1 | 100% |
| Hardcoded secret | 2/2 | 100% |
| Loose regex pattern | 2/2 | 100% |
| npm script changed | 1/1 | 100% |
| Permissive logging config | 1/1 | 100% |
| Removed sanitization | 2/2 | 100% |
| Undeclared import | 1/1 | 100% |
| Weakened cookie flags | 3/3 | 100% |

## Improvement loop

- Improve Diff Drift output so blind reviewers find the same risky nodes with less ambiguity.
- Add harder cases and keep benign cases in the mix so the score cannot rise by always blocking.
- Treat scorer changes as rubric calibration: aliases and accepted decisions should reflect defensible human review, not hide misses.
- Review misses in `missedExpectations` and unmatched findings before changing the product or rubric.
