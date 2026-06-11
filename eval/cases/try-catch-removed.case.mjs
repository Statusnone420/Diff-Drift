export default {
  id: "try-catch-removed",
  title: "Telemetry flush loses its try/catch",
  repo: {
    project: "telemetry",
    branch: "agent/cleanup-flush",
  },
  before: {
    "src/flush.ts": `export function flush(queue: Event[]): void {
  try {
    pushEvents(queue);
  } catch (err) {
    scheduleRetry(queue, err);
  }
}
`,
  },
  after: {
    "src/flush.ts": `export function flush(queue: Event[]): void {
  pushEvents(queue);
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Error handling removed", severity: "low", filePath: "src/flush.ts" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
