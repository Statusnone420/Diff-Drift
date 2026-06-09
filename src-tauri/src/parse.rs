//! TypeScript parsing via tree-sitter into a `Parsed` intermediate tree.
//! Surfaces top-level statements (and one level of function-body statements),
//! mapping tree-sitter node kinds to the display kinds the UI knows.
use tree_sitter::{Node, Parser};

#[derive(Clone)]
pub struct Parsed {
    pub kind: String,
    pub name: String,
    pub signature: Option<String>,
    pub lines: Vec<String>,
    pub children: Vec<Parsed>,
}

impl Parsed {
    /// Stable-ish key for matching a node across the before/after versions.
    pub fn key(&self) -> (String, String) {
        (self.kind.clone(), self.name.clone())
    }
}

/// Parse a TS source string into the top-level `Parsed` nodes.
pub fn parse_file(source: &str) -> Vec<Parsed> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter_map(|n| map_node(n, bytes))
        .collect()
}

/// Map a tree-sitter node to a `Parsed`, or `None` to skip (comments, etc.).
fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
    let ts_kind = node.kind();
    let kind = match ts_kind {
        "import_statement" => "ImportDeclaration",
        "function_declaration" | "generator_function_declaration" => "FunctionDeclaration",
        "lexical_declaration" | "variable_declaration" => "VariableDeclaration",
        "if_statement" => "IfStatement",
        "expression_statement" => "ExpressionStatement",
        "return_statement" => "ReturnStatement",
        "export_statement" => "ExportDeclaration",
        "class_declaration" => "ClassDeclaration",
        "interface_declaration" => "InterfaceDeclaration",
        "type_alias_declaration" => "TypeAliasDeclaration",
        "comment" => return None,
        other => other,
    }
    .to_string();

    let name = node_name(node, src, ts_kind);
    let signature = node_signature(node, src, ts_kind);
    let lines = node_lines(node, src);
    let children = if ts_kind == "function_declaration"
        || ts_kind == "generator_function_declaration"
    {
        function_body_children(node, src)
    } else {
        Vec::new()
    };

    Some(Parsed {
        kind,
        name,
        signature,
        lines,
        children,
    })
}

fn function_body_children(node: Node, src: &[u8]) -> Vec<Parsed> {
    let Some(body) = node.child_by_field_name("body") else {
        return Vec::new();
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter_map(|n| map_node(n, src))
        .collect()
}

fn text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn node_name(node: Node, src: &[u8], ts_kind: &str) -> String {
    match ts_kind {
        "import_statement" => first_descendant_kind(node, "string")
            .map(|s| strip_quotes(text(s, src)).to_string())
            .unwrap_or_else(|| "import".into()),
        "function_declaration" | "generator_function_declaration" => node
            .child_by_field_name("name")
            .map(|n| text(n, src).to_string())
            .unwrap_or_else(|| "function".into()),
        "lexical_declaration" | "variable_declaration" => declarator_name(node, src)
            .unwrap_or_else(|| "declaration".into()),
        "if_statement" => "if".into(),
        "return_statement" => "return".into(),
        "expression_statement" => snippet(node, src, 48),
        "export_statement" => export_name(node, src),
        "class_declaration" | "interface_declaration" | "type_alias_declaration" => node
            .child_by_field_name("name")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default(),
        _ => snippet(node, src, 40),
    }
}

fn node_signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
    if ts_kind == "function_declaration" || ts_kind == "generator_function_declaration" {
        let params = node
            .child_by_field_name("parameters")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        let ret = node
            .child_by_field_name("return_type")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        let sig = format!("{params}{ret}");
        if sig.is_empty() {
            None
        } else {
            Some(sig)
        }
    } else {
        None
    }
}

fn declarator_name(node: Node, src: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(name) = child.child_by_field_name("name") {
                return Some(text(name, src).to_string());
            }
        }
    }
    None
}

fn export_name(node: Node, src: &[u8]) -> String {
    // `export default X` / `export const Y` etc. — surface the inner declaration's name.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "lexical_declaration" | "variable_declaration" => {
                if let Some(n) = declarator_name(child, src) {
                    return n;
                }
            }
            "function_declaration" | "class_declaration" => {
                if let Some(n) = child.child_by_field_name("name") {
                    return text(n, src).to_string();
                }
            }
            "identifier" => return text(child, src).to_string(),
            _ => {}
        }
    }
    "export".into()
}

fn first_descendant_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = first_descendant_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

fn strip_quotes(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

/// First line of a node's text, trailing `;` removed, capped to `max` chars.
fn snippet(node: Node, src: &[u8], max: usize) -> String {
    let first = text(node, src).lines().next().unwrap_or("").trim();
    let first = first.strip_suffix(';').unwrap_or(first);
    if first.chars().count() > max {
        let mut s: String = first.chars().take(max).collect();
        s.push('…');
        s
    } else {
        first.to_string()
    }
}

/// Source lines of a node, dedented so the block displays cleanly (the first
/// line already starts at the node; later lines are stripped of the node's
/// column-worth of leading whitespace).
fn node_lines(node: Node, src: &[u8]) -> Vec<String> {
    let t = text(node, src);
    let col = node.start_position().column;
    t.lines()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                l.to_string()
            } else {
                let leading = l.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                l.chars().skip(leading.min(col)).collect()
            }
        })
        .collect()
}
