// An agent "cleans up" a config loader by dropping the propagating `?` and
// finishing with `.unwrap()` — a failure that was handled now panics the
// process at runtime.
export default {
  id: "rust-error-handling-removed",
  title: "Rust fallible result replaced with a panicking unwrap",
  repo: {
    project: "config-loader",
    branch: "agent/tidy-config",
  },
  before: {
    "src/config.rs": `pub fn load(path: &Path) -> Result<Config, Error> {
    let raw = read_to_string(path)?;
    parse_config(&raw)
}
`,
  },
  after: {
    "src/config.rs": `pub fn load(path: &Path) -> Config {
    let raw = read_file(path);
    parse_config(&raw).unwrap()
}
`,
  },
  oracle: {
    expectedExitCode: 1,
    changedFiles: 1,
    riskCount: 1,
    requiredFlags: [{ type: "Error handling removed", severity: "low", filePath: "src/config.rs" }],
  },
  agent: {
    expectedDecision: "investigate",
  },
};
