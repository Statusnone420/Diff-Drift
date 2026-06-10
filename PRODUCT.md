# Product

## Register

product

## Users

Diff Drift is for developers and technical reviewers using AI coding agents on local repositories. They open it after an agent changes code and need a fast, deterministic second pass before they trust, merge, or release the work.

## Product Purpose

Diff Drift compares local git drift against a selected baseline, renders structural changes for TS/TSX/JS/JSX plus package.json dependency drift, tracks per-node review state, and raises heuristic security flags for human review. Success means a reviewer can quickly see what changed, what looks risky, and what still needs review before trust.

## Brand Personality

Deterministic, local, pragmatic. The product should feel like a precise reviewer in the loop, not a broad static analyzer, cloud service, or LLM assistant.

## Anti-references

Avoid cloud-first review dashboards, rule marketplaces, team-account workflows, telemetry, model calls, exaggerated vulnerability claims, and mini-SAST depth chasing. Avoid marketing gloss that makes heuristic review prompts sound like vulnerability verdicts.

## Design Principles

- Make risky drift easier to find than harmless noise.
- Keep scope visible: local, deterministic, drift-focused.
- Preserve reviewer control with clear baselines, review state, and exportable evidence.
- Favor dense, scannable product UI over decorative storytelling.
- Treat benchmark and release claims as evidence with clear limitations.

## Accessibility & Inclusion

Use readable contrast, keyboard-reachable controls, visible focus states, and reduced-motion-safe interactions. Browser E2E includes axe checks, and generated reports should remain understandable when printed or viewed as static HTML.
