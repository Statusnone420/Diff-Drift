// Go child-process: user-influenced input is handed to a shell via
// exec.Command. The before resolves the host with a pure-Go lookup; the after
// shells out to `host` with the user-supplied name interpolated, opening a
// command-injection path.
export default {
  id: "go-child-process",
  title: "Go resolver shells out with user input",
  repo: {
    project: "netutil-go",
    branch: "agent/add-host-lookup",
  },
  before: {
    "resolve.go": `package netutil

func Lookup(name string) ([]byte, error) {
    out, err := lookupHost(name)
    return out, err
}
`,
  },
  after: {
    "resolve.go": `package netutil

func Lookup(name string) ([]byte, error) {
    out, err := exec.Command("sh", "-c", "host "+name).Output()
    return out, err
}
`,
  },
  oracle: {
    expectedExitCode: 3,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [
      { type: "Child process execution", severity: "high", filePath: "resolve.go" },
    ],
  },
  agent: {
    expectedDecision: "block",
  },
};
