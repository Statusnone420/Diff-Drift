export default {
  id: "dynamic-code-execution",
  title: "Runtime expression evaluator introduced",
  repo: {
    project: "billing-worker",
    branch: "agent/add-expression-support",
  },
  before: {
    "src/renderExpression.ts": `export function renderExpression(input: string): string {
  return input.trim();
}
`,
  },
  after: {
    "src/renderExpression.ts": `export function renderExpression(input: string): string {
  return eval(input);
}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Dynamic code execution", severity: "high", filePath: "src/renderExpression.ts" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
