//! TypeScript parsing via tree-sitter into a `Parsed` intermediate tree.
//! Surfaces top-level statements (and one level of function-body statements),
//! mapping tree-sitter node kinds to the display kinds the UI knows.
use tree_sitter::{Node, Parser};

#[derive(Clone, Debug)]
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

/// The source languages Diff Drift parses as AST drift.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ts,
    Tsx,
    Js,
    Jsx,
}

impl Lang {
    /// Language for a repo-relative path, or `None` when the file isn't parsed
    /// as AST drift (`.d.ts` and everything non-JS/TS).
    pub fn from_path(rel: &str) -> Option<Lang> {
        let p = rel.to_ascii_lowercase();
        if p.ends_with(".d.ts") {
            return None;
        }
        if p.ends_with(".ts") {
            Some(Lang::Ts)
        } else if p.ends_with(".tsx") {
            Some(Lang::Tsx)
        } else if p.ends_with(".jsx") {
            Some(Lang::Jsx)
        } else if p.ends_with(".js") || p.ends_with(".mjs") || p.ends_with(".cjs") {
            Some(Lang::Js)
        } else {
            None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Ts => "TypeScript",
            Lang::Tsx => "TSX",
            Lang::Js => "JavaScript",
            Lang::Jsx => "JSX",
        }
    }
}

/// The tree-sitter grammar for a language. The TSX grammar is selected for
/// `.tsx` (JSX is a parse ERROR under plain TypeScript); the JavaScript grammar
/// handles JSX natively for `.js/.jsx/.mjs/.cjs`.
pub fn grammar(lang: Lang) -> tree_sitter::Language {
    match lang {
        Lang::Ts => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Lang::Js | Lang::Jsx => tree_sitter_javascript::LANGUAGE.into(),
    }
}

/// Parse a source string into the top-level `Parsed` nodes. The TSX grammar is
/// selected for `.tsx` (JSX is a parse ERROR under plain TypeScript); the
/// JavaScript grammar handles JSX natively for `.js/.jsx/.mjs/.cjs`.
pub fn parse_file(source: &str, lang: Lang) -> Vec<Parsed> {
    let mut parser = Parser::new();
    if parser.set_language(&grammar(lang)).is_err() {
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

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"import { decode } from "jwt-tiny-decode";

const pattern = /.*/;

function validateToken(token: string): boolean {
  if (!pattern.test(token)) {
    throw new Error("nope");
  }
  return decode(token);
}

export default router;
"#;

    #[test]
    fn maps_kinds_names_and_signatures() {
        let nodes = parse_file(SRC, Lang::Ts);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("ImportDeclaration", "jwt-tiny-decode"),
                ("VariableDeclaration", "pattern"),
                ("FunctionDeclaration", "validateToken"),
                ("ExportDeclaration", "router"),
            ]
        );
        let func = &nodes[2];
        assert_eq!(func.signature.as_deref(), Some("(token: string): boolean"));
    }

    #[test]
    fn function_bodies_surface_one_level_of_children() {
        let nodes = parse_file(SRC, Lang::Ts);
        let func = &nodes[2];
        let child_kinds: Vec<&str> = func.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(child_kinds, vec!["IfStatement", "ReturnStatement"]);
        assert_eq!(func.children[1].lines, vec!["return decode(token);"]);
    }

    #[test]
    fn unparseable_or_empty_source_yields_no_nodes() {
        assert!(parse_file("", Lang::Ts).is_empty());
        // tree-sitter is error-tolerant; garbage shouldn't panic.
        let _ = parse_file("@@@ ??? not typescript {{{", Lang::Ts);
    }

    #[test]
    fn multiline_nodes_are_dedented() {
        let src = "function f() {\n  const x = {\n    a: 1,\n  };\n}\n";
        let nodes = parse_file(src, Lang::Ts);
        let decl = &nodes[0].children[0];
        assert_eq!(decl.lines, vec!["const x = {", "  a: 1,", "};"]);
    }

    #[test]
    fn tsx_source_parses_with_the_tsx_grammar() {
        // Under the TS grammar this JSX only parses via error recovery (inner
        // ERROR nodes); the TSX grammar parses it as real syntax. The surfaced
        // skeleton happens to survive recovery for simple components, but the
        // guarantee we want — JSX is valid syntax, not tolerated garbage — only
        // holds with the TSX grammar.
        let src = "function Badge({ label }: { label: string }) {\n  return <span className=\"badge\">{label}</span>;\n}\n\nconst App = () => <Badge label=\"hi\" />;\n";
        let nodes = parse_file(src, Lang::Tsx);
        let kinds: Vec<(&str, &str)> = nodes.iter().map(|n| (n.kind.as_str(), n.name.as_str())).collect();
        assert_eq!(
            kinds,
            vec![("FunctionDeclaration", "Badge"), ("VariableDeclaration", "App")],
            "JSX parses to clean top-level nodes"
        );
        let child_kinds: Vec<&str> = nodes[0].children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(child_kinds, vec!["ReturnStatement"], "JSX body parses cleanly");
    }

    #[test]
    fn javascript_and_jsx_parse_with_the_js_grammar() {
        let src = "const config = { redact: [] };\n\nfunction handler(req, res) {\n  return res.json({ ok: true });\n}\n\nmodule.exports = handler;\n";
        let nodes = parse_file(src, Lang::Js);
        let kinds: Vec<(&str, &str)> = nodes.iter().map(|n| (n.kind.as_str(), n.name.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                ("VariableDeclaration", "config"),
                ("FunctionDeclaration", "handler"),
                ("ExpressionStatement", "module.exports = handler"),
            ]
        );
        assert_eq!(nodes[1].signature.as_deref(), Some("(req, res)"));

        // JSX is native syntax in the JS grammar.
        let jsx = "function Badge({ label }) {\n  return <span className=\"badge\">{label}</span>;\n}\n";
        let nodes = parse_file(jsx, Lang::Jsx);
        assert_eq!(nodes[0].kind, "FunctionDeclaration");
        assert_eq!(nodes[0].name, "Badge");
        assert_eq!(nodes[0].children[0].kind, "ReturnStatement");
    }

    #[test]
    fn lang_from_path_covers_the_supported_extensions() {
        assert_eq!(Lang::from_path("a.ts"), Some(Lang::Ts));
        assert_eq!(Lang::from_path("a.tsx"), Some(Lang::Tsx));
        assert_eq!(Lang::from_path("a.js"), Some(Lang::Js));
        assert_eq!(Lang::from_path("a.jsx"), Some(Lang::Jsx));
        assert_eq!(Lang::from_path("a.mjs"), Some(Lang::Js));
        assert_eq!(Lang::from_path("a.cjs"), Some(Lang::Js));
        assert_eq!(Lang::from_path("a.d.ts"), None);
        assert_eq!(Lang::from_path("package.json"), None);
        assert_eq!(Lang::from_path("README.md"), None);
    }
}
