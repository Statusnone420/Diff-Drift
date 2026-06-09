//! Report rendering for "Export report": a Markdown summary of the session
//! (suitable for a PR comment / handing back to an AI agent) or the raw
//! `SessionData` as pretty JSON. Pure functions — the command in `lib.rs` writes.
use crate::model::{AstNode, FileEntry, Flag, SessionData, Severity};

pub fn render_json(data: &SessionData) -> String {
    serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".into())
}

pub fn render_markdown(data: &SessionData, generated_at: &str) -> String {
    let s = &data.session;
    let mut out = String::new();
    out.push_str(&format!("# Diff Drift report — {}\n\n", s.project));
    out.push_str(&format!("- **Branch:** `{}`\n", s.branch));
    out.push_str(&format!("- **Repository:** `{}`\n", s.repo_path));
    out.push_str(&format!("- **Generated:** {generated_at}\n"));
    out.push_str(&format!(
        "- **Drift:** {} changed file{} · {} active risk{}\n",
        s.changed_files,
        plural(s.changed_files),
        s.risk_count,
        plural(s.risk_count)
    ));
    out.push_str(&match (&s.approved, &s.approved_at) {
        (true, Some(at)) => format!("- **Approval:** approved at {at}\n"),
        (true, None) => "- **Approval:** approved\n".to_string(),
        _ => "- **Approval:** not approved\n".to_string(),
    });
    out.push('\n');

    let active: Vec<&Flag> = data.flags.iter().filter(|f| !f.dismissed).collect();
    let dismissed: Vec<&Flag> = data.flags.iter().filter(|f| f.dismissed).collect();

    if active.is_empty() {
        out.push_str("## Risk flags\n\nNo active risk flags in this drift.\n\n");
    } else {
        out.push_str("## Risk flags\n\n");
        for sev in [Severity::High, Severity::Medium, Severity::Low] {
            let in_sev: Vec<&&Flag> = active.iter().filter(|f| sev_eq(f.severity, sev)).collect();
            if in_sev.is_empty() {
                continue;
            }
            out.push_str(&format!("### {} severity\n\n", sev_label(sev)));
            for f in in_sev {
                render_flag(&mut out, f, &data.files);
            }
        }
    }

    if !dismissed.is_empty() {
        out.push_str(&format!("## Dismissed ({})\n\n", dismissed.len()));
        for f in &dismissed {
            out.push_str(&format!(
                "- ~~{}~~ — `{}` ({})\n",
                f.r#type, f.file_path, f.node_path
            ));
        }
        out.push('\n');
    }

    out.push_str("## Changed files\n\n");
    if data.files.is_empty() {
        out.push_str("No analyzable TypeScript/TSX drift.\n");
    } else {
        for file in &data.files {
            out.push_str(&format!(
                "- `{}{}` — {} · {} active risk{}\n",
                file.dir,
                file.name,
                file.summary,
                file.risks,
                plural(file.risks)
            ));
        }
    }
    out
}

fn render_flag(out: &mut String, f: &Flag, files: &[FileEntry]) {
    out.push_str(&format!("#### {} — `{}`\n\n", f.r#type, f.file_path));
    out.push_str(&format!("- **Node:** {}\n", f.node_path));
    out.push_str(&format!("- **Detail:** {}\n", f.desc));
    if let Some(node) = files
        .iter()
        .find(|file| file.id == f.file_id)
        .and_then(|file| find_node(&file.nodes, &f.node_id))
    {
        if let Some(before) = node.before.as_ref().filter(|l| !l.is_empty()) {
            out.push_str("\n```diff\n");
            for line in before {
                out.push_str(&format!("- {line}\n"));
            }
            for line in node.after.iter().flatten() {
                out.push_str(&format!("+ {line}\n"));
            }
            out.push_str("```\n");
        } else if let Some(after) = node.after.as_ref().filter(|l| !l.is_empty()) {
            out.push_str("\n```diff\n");
            for line in after {
                out.push_str(&format!("+ {line}\n"));
            }
            out.push_str("```\n");
        }
    }
    out.push('\n');
}

fn find_node<'a>(nodes: &'a [AstNode], id: &str) -> Option<&'a AstNode> {
    for n in nodes {
        if n.id == id {
            return Some(n);
        }
        if let Some(found) = n.children.as_deref().and_then(|c| find_node(c, id)) {
            return Some(found);
        }
    }
    None
}

fn sev_eq(a: Severity, b: Severity) -> bool {
    matches!(
        (a, b),
        (Severity::High, Severity::High)
            | (Severity::Medium, Severity::Medium)
            | (Severity::Low, Severity::Low)
    )
}

fn sev_label(s: Severity) -> &'static str {
    match s {
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
    }
}

fn plural(n: u32) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{analyze_all, assemble, fingerprint, meta};
    use crate::store::RepoState;
    use crate::test_fixture;
    use crate::git;

    fn fixture_data(state: &RepoState) -> SessionData {
        let fixture = test_fixture::payments_api();
        let root = git::repo_root(&fixture.root).unwrap();
        let results = analyze_all(&root);
        let mut state = state.clone();
        if state.approved_fingerprint.as_deref() == Some("CURRENT") {
            state.approved_fingerprint = Some(fingerprint(&results));
        }
        assemble(&results, &meta(&root), &state)
    }

    #[test]
    fn markdown_report_covers_flags_files_and_diffs() {
        let md = render_markdown(&fixture_data(&RepoState::default()), "2026-06-09 12:30");
        assert!(md.contains("# Diff Drift report —"), "title: {md}");
        assert!(md.contains("- **Generated:** 2026-06-09 12:30"));
        assert!(md.contains("- **Branch:** `agent/refactor-token-validation`"));
        assert!(md.contains("- **Approval:** not approved"));
        assert!(md.contains("### High severity"));
        assert!(md.contains("Loose regex pattern"));
        assert!(md.contains("```diff"), "flagged nodes include before/after diff");
        assert!(md.contains("- const pattern ="), "before lines render as removals");
        assert!(md.contains("+ const pattern = /.*/;"), "after lines render as additions");
        assert!(md.contains("## Changed files"));
        assert!(md.contains("`routes/session.ts` — Formatting only"));
        assert!(!md.contains("## Dismissed"), "no dismissed section when nothing dismissed");
    }

    #[test]
    fn markdown_report_shows_dismissed_and_approval() {
        let base = fixture_data(&RepoState::default());
        let mut state = RepoState::default();
        state.dismissed.insert(base.flags[0].id.clone());
        state.approved_fingerprint = Some("CURRENT".into());
        state.approved_at = Some("12:30".into());
        let md = render_markdown(&fixture_data(&state), "now");
        assert!(md.contains("- **Approval:** approved at 12:30"));
        assert!(md.contains("## Dismissed (1)"));
        assert!(md.contains("~~Loose regex pattern~~"));
        assert!(md.contains("5 active risks"));
    }

    #[test]
    fn json_report_is_valid_and_complete() {
        let json = render_json(&fixture_data(&RepoState::default()));
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(v["session"]["riskCount"], 6);
        assert_eq!(v["flags"].as_array().unwrap().len(), 6);
        assert_eq!(v["session"]["approved"], false);
    }
}
