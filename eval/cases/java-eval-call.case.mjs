// An agent wires a rules field straight into a JavaScript ScriptEngine, turning
// stored data into executable code — a dynamic-code-execution path.
export default {
  id: "java-eval-call",
  title: "Java ScriptEngine evaluates user-supplied expression",
  repo: {
    project: "rules-engine",
    branch: "agent/dynamic-rules",
  },
  before: {
    "src/main/java/rules/Evaluator.java": `package rules;

class Evaluator {
    Object run(String expression) {
        return staticEvaluate(expression);
    }
}
`,
  },
  after: {
    "src/main/java/rules/Evaluator.java": `package rules;

import javax.script.ScriptEngine;
import javax.script.ScriptEngineManager;

class Evaluator {
    Object run(String expression) {
        ScriptEngine engine = new ScriptEngineManager().getEngineByName("js");
        return engine.eval(expression);
    }
}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Dynamic code execution", severity: "high", filePath: "src/main/java/rules/Evaluator.java" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
