# Diff Drift — blind-agent multi-model panel (benchmark v4)

Generated: 2026-06-11T02:09:51.276Z

> The models are rulers; Diff Drift is the object being measured. Read the per-model scores and the spread — not a pooled average. An all-model panel stays **independent external validation pending** (that needs a human outside the project).

**Spread: 91–99 / 100** across 3 models.

| Model | Vendor | Overall | Decision acc | Recall | Cases |
| --- | --- | ---: | ---: | ---: | ---: |
| Claude Opus 4.8 | Anthropic | 99/100 | 95% | 100% | 20 |
| Claude Sonnet 4.6 | Anthropic | 99/100 | 95% | 100% | 20 |
| Claude Haiku 4.5 | Anthropic | 91/100 | 95% | 90% | 20 |

## Model × case matrix

| Case | Claude Opus 4.8 | Claude Sonnet 4.6 | Claude Haiku 4.5 | Agreement |
| --- | ---: | ---: | ---: | --- |
| test-file-hardcoded-secret | 100 | 100 | 0 | split |
| child-process-execution | 100 | 100 | 15 | split |
| try-catch-removed | 80 | 80 | 100 | split |
| payments-api-auth-regression | 90 | 100 | 100 | split |
| benign-eval-in-string | 100 | 100 | 100 | ✓ clean |
| benign-formatting-only | 100 | 100 | 100 | ✓ clean |
| broadened-cors | 100 | 100 | 100 | ✓ clean |
| constant-falsy-guard-evasion | 100 | 100 | 100 | ✓ clean |
| disabled-tls-verification | 100 | 100 | 100 | ✓ clean |
| dynamic-code-execution | 100 | 100 | 100 | ✓ clean |
| guard-removed-around-call | 100 | 100 | 100 | ✓ clean |
| hardcoded-secret | 100 | 100 | 100 | ✓ clean |
| jsx-hardcoded-secret | 100 | 100 | 100 | ✓ clean |
| mjs-disabled-tls | 100 | 100 | 100 | ✓ clean |
| noisy-benign-refactor | 100 | 100 | 100 | ✓ clean |
| oversized-file-skip | 100 | 100 | 100 | ✓ clean |
| package-dependency-script-drift | 100 | 100 | 100 | ✓ clean |
| regex-anchors-removed | 100 | 100 | 100 | ✓ clean |
| tsx-removed-sanitization | 100 | 100 | 100 | ✓ clean |
| weakened-cookie-flags | 100 | 100 | 100 | ✓ clean |

**Product-clarity signals:** none — no case is missed by every model.

**Splits** (models disagree — treat as ruler noise, not a tool defect): test-file-hardcoded-secret, child-process-execution, try-catch-removed, payments-api-auth-regression.
