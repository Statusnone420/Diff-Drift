// An agent removes the try/catch that wrapped a network send while keeping the
// send itself, so a failure that used to be logged is now unhandled.
export default {
  id: "java-error-handling-removed",
  title: "Java try/catch around send removed",
  repo: {
    project: "notify-svc",
    branch: "agent/inline-send",
  },
  before: {
    "src/main/java/notify/Sender.java": `package notify;

class Sender {
    void deliver(Message payload) {
        try {
            transport.send(payload);
        } catch (IOException e) {
            log.warn("send failed", e);
        }
    }
}
`,
  },
  after: {
    "src/main/java/notify/Sender.java": `package notify;

class Sender {
    void deliver(Message payload) {
        transport.send(payload);
    }
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Error handling removed", severity: "low", filePath: "src/main/java/notify/Sender.java" },
    ],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
