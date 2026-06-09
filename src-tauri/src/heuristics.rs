//! Walks a file's diffed node tree, applies the rule registry, and attaches a
//! stable `flag_id` (`{ruleId}@{nodeId}`) to each flagged node. Ids must already be
//! assigned (`diff::assign_ids`) before calling this.
use crate::model::{AstNode, Flag};
use crate::rules::{RuleCtx, RuleRegistry};

pub fn scan_file(
    file_id: &str,
    file_path: &str,
    nodes: &mut [AstNode],
    parent: Option<&str>,
    flags: &mut Vec<Flag>,
    registry: &RuleRegistry,
    ctx: &RuleCtx,
) {
    for node in nodes.iter_mut() {
        if let Some((rule_id, f)) = registry.check(node, ctx) {
            let flag_id = format!("{rule_id}@{}", node.id);
            node.flag_id = Some(flag_id.clone());
            flags.push(Flag {
                id: flag_id,
                severity: f.severity,
                r#type: f.r#type.to_string(),
                desc: f.desc,
                file_id: file_id.to_string(),
                file_path: file_path.to_string(),
                node_path: match parent {
                    Some(p) => format!("{p} › {}", node.name),
                    None => node.name.clone(),
                },
                node_id: node.id.clone(),
                dismissed: false, // applied later in `session::assemble` from the per-repo store
            });
        }
        if let Some(children) = node.children.as_mut() {
            scan_file(file_id, file_path, children, Some(&node.name), flags, registry, ctx);
        }
    }
}
