export default {
  id: "guard-removed-around-call",
  title: "Payment capture loses its verification guard",
  repo: {
    project: "checkout",
    branch: "agent/simplify-capture",
  },
  before: {
    "src/capture.ts": `export function capture(order: Order): void {
  if (isVerified(order)) {
    chargeCard(order);
  }
}
`,
  },
  after: {
    "src/capture.ts": `export function capture(order: Order): void {
  chargeCard(order);
}
`,
  },
  oracle: {
    expectedExitCode: 2,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Guard removed", severity: "medium", filePath: "src/capture.ts" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
