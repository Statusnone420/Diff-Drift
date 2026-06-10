# Diff Drift blind-agent scorecard

Generated: 2026-06-10T14:35:13.357Z

> Advisory only: this score is not a CI gate. The CI blocker is `npm run eval:engine`; blind-agent scoring measures whether reviewers can use Diff Drift packets to reach the right evidence and decision.

Evaluators: claude-fable-5 (model, 15 cases)

> **Independent external validation pending.** All answers so far come from a single evaluator or an all-model panel. Treat the score as an internal product-quality signal, not third-party validation.

Overall score: [##########] 98/100

- Decision accuracy: 100%
- Finding recall: 99%
- Localization: 100%
- Precision: 86% (18/21 reported findings matched an expected flag; 3 unmatched)

| Case | Score | Decision | Recall | Notes |
| --- | ---: | --- | ---: | --- |
| payments-api-auth-regression | 84 | ok (block) | 90% | missed Disabled guard |
| child-process-execution | 95 | ok (block) | 100% | 1 unmatched |
| jsx-hardcoded-secret | 95 | ok (block) | 100% | 1 unmatched |
| weakened-cookie-flags | 95 | ok (block) | 100% | 1 unmatched |
| benign-formatting-only | 100 | ok (approve) | 100% | clean |
| broadened-cors | 100 | ok (block) | 100% | clean |
| disabled-tls-verification | 100 | ok (block) | 100% | clean |
| dynamic-code-execution | 100 | ok (block) | 100% | clean |
| hardcoded-secret | 100 | ok (block) | 100% | clean |
| mjs-disabled-tls | 100 | ok (block) | 100% | clean |
| noisy-benign-refactor | 100 | ok (approve) | 100% | clean |
| oversized-file-skip | 100 | ok (investigate/approve) | 100% | clean |
| package-dependency-script-drift | 100 | ok (investigate/block) | 100% | clean |
| test-fixture-suppression | 100 | ok (approve) | 100% | clean |
| tsx-removed-sanitization | 100 | ok (investigate/block) | 100% | clean |

## Per-rule recall

Across every case that required the flag type:

| Flag type | Matched / Required | Recall |
| --- | ---: | ---: |
| Broadened CORS | 1/1 | 100% |
| Child process execution | 1/1 | 100% |
| Crypto downgrade | 1/1 | 100% |
| Dependency not in lockfile | 1/1 | 100% |
| Disabled guard | 0/1 | 0% |
| Disabled TLS verification | 2/2 | 100% |
| Dynamic code execution | 1/1 | 100% |
| Hardcoded secret | 2/2 | 100% |
| Loose regex pattern | 1/1 | 100% |
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

