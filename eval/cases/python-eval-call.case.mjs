// A rules engine starts evaluating its expression strings with eval(), turning
// stored expression data into executable code — a code-injection path.
export default {
  id: "python-eval-call",
  title: "Expression engine switches to eval()",
  repo: {
    project: "rules-engine",
    branch: "agent/dynamic-expr",
  },
  before: {
    "engine/expr.py": `def evaluate(expr, context):
    fn = COMPILED.get(expr)
    return fn(context)
`,
  },
  after: {
    "engine/expr.py": `def evaluate(expr, context):
    return eval(expr, {}, context)
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Dynamic code execution", severity: "high", filePath: "engine/expr.py" }],
  },
  agent: {
    expectedDecision: "block",
  },
};
