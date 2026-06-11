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
    Rust,
    Go,
    Python,
    Java,
    CSharp,
    Kotlin,
    Swift,
}

/// Language family. Security rules are written against JS/TS grammar node kinds,
/// so they only run for `JsTs`; every other language gets structural drift
/// (skeletons, function children, before→after diff) plus the genuinely
/// language-neutral hardcoded-secret check, but no JS-specific security rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    JsTs,
    Rust,
    Go,
    Python,
    Java,
    CSharp,
    Kotlin,
    Swift,
}

impl Lang {
    /// Language for a repo-relative path, or `None` when the file isn't parsed
    /// as AST drift (`.d.ts` and everything not in a supported language).
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
        } else if p.ends_with(".rs") {
            Some(Lang::Rust)
        } else if p.ends_with(".go") {
            Some(Lang::Go)
        } else if p.ends_with(".py") || p.ends_with(".pyi") {
            Some(Lang::Python)
        } else if p.ends_with(".java") {
            Some(Lang::Java)
        } else if p.ends_with(".cs") {
            Some(Lang::CSharp)
        } else if p.ends_with(".kt") || p.ends_with(".kts") {
            Some(Lang::Kotlin)
        } else if p.ends_with(".swift") {
            Some(Lang::Swift)
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
            Lang::Rust => "Rust",
            Lang::Go => "Go",
            Lang::Python => "Python",
            Lang::Java => "Java",
            Lang::CSharp => "C#",
            Lang::Kotlin => "Kotlin",
            Lang::Swift => "Swift",
        }
    }

    /// The language family — the single switch the rule layer gates on.
    pub fn family(self) -> Family {
        match self {
            Lang::Ts | Lang::Tsx | Lang::Js | Lang::Jsx => Family::JsTs,
            Lang::Rust => Family::Rust,
            Lang::Go => Family::Go,
            Lang::Python => Family::Python,
            Lang::Java => Family::Java,
            Lang::CSharp => Family::CSharp,
            Lang::Kotlin => Family::Kotlin,
            Lang::Swift => Family::Swift,
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
        Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
        Lang::Go => tree_sitter_go::LANGUAGE.into(),
        Lang::Python => tree_sitter_python::LANGUAGE.into(),
        Lang::Java => tree_sitter_java::LANGUAGE.into(),
        Lang::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Lang::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        Lang::Swift => tree_sitter_swift::LANGUAGE.into(),
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
    match lang.family() {
        Family::JsTs => root
            .named_children(&mut cursor)
            .filter_map(|n| map_node(n, bytes))
            .collect(),
        Family::Rust => root
            .named_children(&mut cursor)
            .filter_map(|n| rust::map_node(n, bytes))
            .collect(),
        Family::Go => root
            .named_children(&mut cursor)
            .filter_map(|n| go::map_node(n, bytes))
            .collect(),
        Family::Python => root
            .named_children(&mut cursor)
            .filter_map(|n| python::map_node(n, bytes))
            .collect(),
        Family::Java => root
            .named_children(&mut cursor)
            .filter_map(|n| java::map_node(n, bytes))
            .collect(),
        Family::CSharp => root
            .named_children(&mut cursor)
            .filter_map(|n| csharp::map_node(n, bytes))
            .collect(),
        Family::Kotlin => root
            .named_children(&mut cursor)
            .filter_map(|n| kotlin::map_node(n, bytes))
            .collect(),
        Family::Swift => root
            .named_children(&mut cursor)
            .filter_map(|n| swift::map_node(n, bytes))
            .collect(),
    }
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

// ===========================================================================
// Per-language mappers for the core-four structural-drift languages.
//
// Each maps its grammar's top-level constructs onto the SAME normalized display
// kinds the JS/TS path uses where the concept lines up (FunctionDeclaration,
// VariableDeclaration, ImportDeclaration, ClassDeclaration, …), so `diff.rs`
// (generic over `(kind, name)`) and the UI need no per-language code. Where
// nothing maps, a language-specific kind string is emitted (`ImplBlock`,
// `Decorator`); the UI renders kind strings generically.
//
// Names come from each grammar's `name` field where present, the import path
// for imports, and the shared `snippet()` fallback otherwise. Function-like
// nodes surface ONE level of body statements, mirroring the JS behavior.
// ===========================================================================

/// Shared helper: the first named child whose kind is one of `kinds`.
fn first_child_of_kinds<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|c| kinds.contains(&c.kind()));
    found
}

/// Map one level of statements inside a function-like `body` block.
fn body_children<F>(node: Node, src: &[u8], map: F) -> Vec<Parsed>
where
    F: Fn(Node, &[u8]) -> Option<Parsed>,
{
    let Some(body) = node.child_by_field_name("body") else {
        return Vec::new();
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter_map(|n| map(n, src))
        .collect()
}

mod rust {
    use super::{
        body_children, first_child_of_kinds, node_lines, snippet, text, Node, Parsed,
    };

    /// Map a Rust node into a `Parsed`, or `None` to skip (comments, attributes).
    pub fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
        let ts_kind = node.kind();
        let kind = match ts_kind {
            "use_declaration" => "ImportDeclaration",
            "function_item" => "FunctionDeclaration",
            // `static`/`const`/`let` are all "a named binding".
            "static_item" | "const_item" | "let_declaration" => "VariableDeclaration",
            // struct/enum/type-alias all declare a named type → ClassDeclaration,
            // the kind the UI already groups "type-shaped" declarations under.
            "struct_item" | "enum_item" | "union_item" | "type_item" => "ClassDeclaration",
            "trait_item" => "InterfaceDeclaration",
            // No JS equivalent — keep the Rust-specific kind; the UI renders it.
            "impl_item" => "ImplBlock",
            "if_expression" => "IfStatement",
            "return_expression" => "ReturnStatement",
            "expression_statement" => "ExpressionStatement",
            "mod_item" => "ModuleDeclaration",
            "macro_definition" => "FunctionDeclaration",
            "line_comment" | "block_comment" | "attribute_item" | "inner_attribute_item" => {
                return None
            }
            other => other,
        }
        .to_string();

        let name = name(node, src, ts_kind);
        let signature = signature(node, src, ts_kind);
        let lines = node_lines(node, src);
        let children = if ts_kind == "function_item" {
            body_children(node, src, map_node)
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

    fn name(node: Node, src: &[u8], ts_kind: &str) -> String {
        match ts_kind {
            "use_declaration" => node
                .child_by_field_name("argument")
                .map(|n| text(n, src).trim().to_string())
                .unwrap_or_else(|| "use".into()),
            // Most items expose a `name` field directly.
            "function_item" | "static_item" | "const_item" | "struct_item" | "enum_item"
            | "union_item" | "type_item" | "trait_item" | "mod_item" | "macro_definition" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_default(),
            // `impl Trait for Type` / `impl Type` — name after the `type` field.
            "impl_item" => node
                .child_by_field_name("type")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| "impl".into()),
            // `let <pattern> = …` — first identifier in the pattern.
            "let_declaration" => first_child_of_kinds(node, &["identifier"])
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| snippet(node, src, 40)),
            _ => snippet(node, src, 40),
        }
    }

    fn signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
        if ts_kind != "function_item" {
            return None;
        }
        let params = node
            .child_by_field_name("parameters")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        let ret = node
            .child_by_field_name("return_type")
            .map(|n| format!(" -> {}", text(n, src)))
            .unwrap_or_default();
        let sig = format!("{params}{ret}");
        if sig.is_empty() {
            None
        } else {
            Some(sig)
        }
    }
}

mod go {
    use super::{body_children, node_lines, snippet, text, Node, Parsed};

    pub fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
        let ts_kind = node.kind();
        let kind = match ts_kind {
            "import_declaration" => "ImportDeclaration",
            "function_declaration" | "method_declaration" => "FunctionDeclaration",
            "const_declaration" | "var_declaration" | "short_var_declaration" => {
                "VariableDeclaration"
            }
            // `type X struct{…}` / `type X interface{…}` both declare a named type.
            "type_declaration" => "ClassDeclaration",
            "if_statement" => "IfStatement",
            "return_statement" => "ReturnStatement",
            "expression_statement" => "ExpressionStatement",
            "package_clause" => "PackageDeclaration",
            "comment" => return None,
            other => other,
        }
        .to_string();

        let name = name(node, src, ts_kind);
        let signature = signature(node, src, ts_kind);
        let lines = node_lines(node, src);
        // Go wraps a function body's statements in a `statement_list`.
        let children = if ts_kind == "function_declaration" || ts_kind == "method_declaration" {
            statement_list_children(node, src)
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

    fn statement_list_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        // `body` is a `block` whose single named child is a `statement_list`.
        let mut bc = body.walk();
        for sl in body.named_children(&mut bc) {
            if sl.kind() == "statement_list" {
                let mut sc = sl.walk();
                return sl
                    .named_children(&mut sc)
                    .filter_map(|n| map_node(n, src))
                    .collect();
            }
        }
        // Some Go versions surface statements directly under `block`.
        body_children(node, src, map_node)
    }

    fn name(node: Node, src: &[u8], ts_kind: &str) -> String {
        match ts_kind {
            "import_declaration" => import_path(node, src),
            "function_declaration" | "method_declaration" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| "func".into()),
            // const/var/type wrap their binding in a `*_spec` child; the spec's
            // first identifier is the declared name.
            "const_declaration" | "var_declaration" | "type_declaration" => spec_name(node, src),
            "short_var_declaration" => node
                .child_by_field_name("left")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| snippet(node, src, 40)),
            _ => snippet(node, src, 40),
        }
    }

    fn spec_name(node: Node, src: &[u8]) -> String {
        let mut cursor = node.walk();
        for spec in node.named_children(&mut cursor) {
            match spec.kind() {
                "const_spec" | "var_spec" => {
                    let mut sc = spec.walk();
                    let id = spec
                        .named_children(&mut sc)
                        .find(|c| c.kind() == "identifier");
                    if let Some(id) = id {
                        return text(id, src).to_string();
                    }
                }
                "type_spec" => {
                    if let Some(n) = spec.child_by_field_name("name") {
                        return text(n, src).to_string();
                    }
                    let mut sc = spec.walk();
                    let id = spec
                        .named_children(&mut sc)
                        .find(|c| c.kind() == "type_identifier");
                    if let Some(id) = id {
                        return text(id, src).to_string();
                    }
                }
                _ => {}
            }
        }
        snippet(node, src, 40)
    }

    fn import_path(node: Node, src: &[u8]) -> String {
        // First interpreted_string_literal anywhere under the import = the path.
        fn find_str<'a>(n: Node<'a>) -> Option<Node<'a>> {
            let mut c = n.walk();
            for ch in n.named_children(&mut c) {
                if ch.kind() == "interpreted_string_literal" || ch.kind() == "raw_string_literal" {
                    return Some(ch);
                }
                if let Some(f) = find_str(ch) {
                    return Some(f);
                }
            }
            None
        }
        find_str(node)
            .map(|s| {
                text(s, src)
                    .trim_matches(|c| c == '"' || c == '`')
                    .to_string()
            })
            .unwrap_or_else(|| "import".into())
    }

    fn signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
        if ts_kind != "function_declaration" && ts_kind != "method_declaration" {
            return None;
        }
        let params = node
            .child_by_field_name("parameters")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        let result = node
            .child_by_field_name("result")
            .map(|n| format!(" {}", text(n, src)))
            .unwrap_or_default();
        let sig = format!("{params}{result}");
        if sig.is_empty() {
            None
        } else {
            Some(sig)
        }
    }
}

mod python {
    use super::{body_children, node_lines, snippet, text, Node, Parsed};

    pub fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
        // A decorated def/class wraps the real definition — unwrap to it so the
        // skeleton shows the function/class, not a `Decorator` shell.
        if node.kind() == "decorated_definition" {
            if let Some(def) = node.child_by_field_name("definition") {
                return map_node(def, src);
            }
        }
        let ts_kind = node.kind();
        let kind = match ts_kind {
            "import_statement" | "import_from_statement" | "future_import_statement" => {
                "ImportDeclaration"
            }
            "function_definition" => "FunctionDeclaration",
            "class_definition" => "ClassDeclaration",
            "if_statement" => "IfStatement",
            "return_statement" => "ReturnStatement",
            // A top-level `X = …` assignment is a named binding.
            "expression_statement" if is_assignment(node) => "VariableDeclaration",
            "expression_statement" => "ExpressionStatement",
            "comment" => return None,
            other => other,
        }
        .to_string();

        let name = name(node, src, ts_kind);
        let signature = signature(node, src, ts_kind);
        let lines = node_lines(node, src);
        let children = if ts_kind == "function_definition" {
            body_children(node, src, map_node)
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

    fn is_assignment(node: Node) -> bool {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .any(|ch| ch.kind() == "assignment" || ch.kind() == "augmented_assignment");
        found
    }

    fn name(node: Node, src: &[u8], ts_kind: &str) -> String {
        match ts_kind {
            "import_statement" | "future_import_statement" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| dotted_after(node, src)),
            "import_from_statement" => node
                .child_by_field_name("module_name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| "import".into()),
            "function_definition" | "class_definition" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_default(),
            "expression_statement" => assignment_target(node, src)
                .unwrap_or_else(|| snippet(node, src, 48)),
            _ => snippet(node, src, 40),
        }
    }

    fn dotted_after(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "dotted_name" || ch.kind() == "aliased_import");
        found
            .map(|n| text(n, src).to_string())
            .unwrap_or_else(|| "import".into())
    }

    fn assignment_target(node: Node, src: &[u8]) -> Option<String> {
        let mut c = node.walk();
        let assign = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "assignment" || ch.kind() == "augmented_assignment")?;
        // The LHS is the assignment's first named child (identifier or pattern_list).
        let mut ac = assign.walk();
        let lhs = assign.named_children(&mut ac).next()?;
        Some(text(lhs, src).to_string())
    }

    fn signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
        if ts_kind != "function_definition" {
            return None;
        }
        let params = node
            .child_by_field_name("parameters")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        let ret = node
            .child_by_field_name("return_type")
            .map(|n| format!(" -> {}", text(n, src)))
            .unwrap_or_default();
        let sig = format!("{params}{ret}");
        if sig.is_empty() {
            None
        } else {
            Some(sig)
        }
    }
}

mod java {
    use super::{node_lines, snippet, text, Node, Parsed};

    pub fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
        let ts_kind = node.kind();
        let kind = match ts_kind {
            "import_declaration" => "ImportDeclaration",
            "package_declaration" => "PackageDeclaration",
            "class_declaration" | "record_declaration" => "ClassDeclaration",
            "interface_declaration" | "annotation_type_declaration" => "InterfaceDeclaration",
            "enum_declaration" => "ClassDeclaration",
            "method_declaration" | "constructor_declaration" => "FunctionDeclaration",
            "field_declaration" | "local_variable_declaration" => "VariableDeclaration",
            "if_statement" => "IfStatement",
            "return_statement" => "ReturnStatement",
            "expression_statement" => "ExpressionStatement",
            "line_comment" | "block_comment" => return None,
            other => other,
        }
        .to_string();

        let name = name(node, src, ts_kind);
        let signature = signature(node, src, ts_kind);
        let lines = node_lines(node, src);
        let children = match ts_kind {
            // Surface the type's members (methods/fields) as one level of children.
            "class_declaration" | "interface_declaration" | "enum_declaration"
            | "record_declaration" | "annotation_type_declaration" => container_children(node, src),
            // Surface a method's body statements.
            "method_declaration" | "constructor_declaration" => method_children(node, src),
            _ => Vec::new(),
        };
        Some(Parsed {
            kind,
            name,
            signature,
            lines,
            children,
        })
    }

    fn container_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        let mut c = body.walk();
        body.named_children(&mut c)
            .filter_map(|n| map_node(n, src))
            .collect()
    }

    fn method_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        let mut c = body.walk();
        body.named_children(&mut c)
            .filter_map(|n| map_node(n, src))
            .collect()
    }

    fn name(node: Node, src: &[u8], ts_kind: &str) -> String {
        match ts_kind {
            "import_declaration" | "package_declaration" => first_scoped_or_identifier(node, src),
            "class_declaration" | "interface_declaration" | "enum_declaration"
            | "record_declaration" | "annotation_type_declaration" | "method_declaration"
            | "constructor_declaration" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_default(),
            "field_declaration" | "local_variable_declaration" => declarator_name(node, src)
                .unwrap_or_else(|| snippet(node, src, 40)),
            _ => snippet(node, src, 40),
        }
    }

    fn first_scoped_or_identifier(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| matches!(ch.kind(), "scoped_identifier" | "identifier"));
        found
            .map(|n| text(n, src).to_string())
            .unwrap_or_else(|| "import".into())
    }

    fn declarator_name(node: Node, src: &[u8]) -> Option<String> {
        let mut c = node.walk();
        for ch in node.named_children(&mut c) {
            if ch.kind() == "variable_declarator" {
                if let Some(n) = ch.child_by_field_name("name") {
                    return Some(text(n, src).to_string());
                }
            }
        }
        None
    }

    fn signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
        if ts_kind != "method_declaration" && ts_kind != "constructor_declaration" {
            return None;
        }
        let params = node
            .child_by_field_name("parameters")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        if params.is_empty() {
            None
        } else {
            Some(params)
        }
    }
}

mod csharp {
    use super::{node_lines, snippet, text, Node, Parsed};

    pub fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
        let ts_kind = node.kind();
        let kind = match ts_kind {
            "using_directive" => "ImportDeclaration",
            "namespace_declaration" | "file_scoped_namespace_declaration" => "PackageDeclaration",
            // class/struct/record/enum all declare a named type.
            "class_declaration" | "struct_declaration" | "record_declaration"
            | "record_struct_declaration" | "enum_declaration" => "ClassDeclaration",
            "interface_declaration" => "InterfaceDeclaration",
            "method_declaration" | "constructor_declaration" | "destructor_declaration"
            | "local_function_statement" => "FunctionDeclaration",
            "field_declaration" | "property_declaration" | "local_declaration_statement" => {
                "VariableDeclaration"
            }
            "if_statement" => "IfStatement",
            "return_statement" => "ReturnStatement",
            "expression_statement" => "ExpressionStatement",
            // Top-level statements are wrapped in a `global_statement`; unwrap to
            // the inner statement so the skeleton shows the real construct.
            "global_statement" => {
                let inner = node.named_child(0)?;
                return map_node(inner, src);
            }
            "comment" => return None,
            other => other,
        }
        .to_string();

        let name = name(node, src, ts_kind);
        let signature = signature(node, src, ts_kind);
        let lines = node_lines(node, src);
        let children = match ts_kind {
            // Surface the type's members (methods/fields) as one level of children.
            "class_declaration" | "struct_declaration" | "record_declaration"
            | "record_struct_declaration" | "enum_declaration" | "interface_declaration" => {
                container_children(node, src)
            }
            // Surface a method's body statements.
            "method_declaration" | "constructor_declaration" | "destructor_declaration"
            | "local_function_statement" => method_children(node, src),
            _ => Vec::new(),
        };
        Some(Parsed {
            kind,
            name,
            signature,
            lines,
            children,
        })
    }

    /// Members live in the `body` `declaration_list`.
    fn container_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        let mut c = body.walk();
        body.named_children(&mut c)
            .filter_map(|n| map_node(n, src))
            .collect()
    }

    /// A method body is a `block`.
    fn method_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        let mut c = body.walk();
        body.named_children(&mut c)
            .filter_map(|n| map_node(n, src))
            .collect()
    }

    fn name(node: Node, src: &[u8], ts_kind: &str) -> String {
        match ts_kind {
            // `using System;` / `using static System.Math;` / `using Foo = X;`
            // The plain form has no `name` field (that's the alias); use the
            // qualified path child. The alias form exposes `name`.
            "using_directive" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| qualified_or_identifier(node, src)),
            "namespace_declaration" | "file_scoped_namespace_declaration" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| "namespace".into()),
            "class_declaration" | "struct_declaration" | "record_declaration"
            | "record_struct_declaration" | "enum_declaration" | "interface_declaration"
            | "method_declaration" | "constructor_declaration" | "local_function_statement" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_default(),
            "field_declaration" | "property_declaration" | "local_declaration_statement" => {
                declarator_name(node, src).unwrap_or_else(|| snippet(node, src, 40))
            }
            _ => snippet(node, src, 40),
        }
    }

    /// The qualified path or bare identifier inside a `using` directive.
    fn qualified_or_identifier(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| matches!(ch.kind(), "qualified_name" | "identifier"));
        found
            .map(|n| text(n, src).to_string())
            .unwrap_or_else(|| "using".into())
    }

    /// `field`/`property`/`local` declarations carry a `variable_declaration`
    /// whose `variable_declarator` exposes the name; a property declares its
    /// name directly.
    fn declarator_name(node: Node, src: &[u8]) -> Option<String> {
        // property_declaration: name field on the node itself.
        if let Some(n) = node.child_by_field_name("name") {
            return Some(text(n, src).to_string());
        }
        let mut c = node.walk();
        let vd = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "variable_declaration")?;
        let mut vc = vd.walk();
        for decl in vd.named_children(&mut vc) {
            if decl.kind() == "variable_declarator" {
                if let Some(n) = decl.child_by_field_name("name") {
                    return Some(text(n, src).to_string());
                }
                // The declarator's first identifier is the name.
                let mut dc = decl.walk();
                let id = decl
                    .named_children(&mut dc)
                    .find(|c| c.kind() == "identifier");
                if let Some(id) = id {
                    return Some(text(id, src).to_string());
                }
            }
        }
        None
    }

    fn signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
        if !matches!(
            ts_kind,
            "method_declaration" | "constructor_declaration" | "local_function_statement"
        ) {
            return None;
        }
        let params = node
            .child_by_field_name("parameters")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        let ret = node
            .child_by_field_name("returns")
            .map(|n| format!(": {}", text(n, src)))
            .unwrap_or_default();
        let sig = format!("{params}{ret}");
        if sig.is_empty() {
            None
        } else {
            Some(sig)
        }
    }
}

mod kotlin {
    use super::{node_lines, snippet, text, Node, Parsed};

    pub fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
        let ts_kind = node.kind();
        let kind = match ts_kind {
            "import" | "import_header" => "ImportDeclaration",
            "package_header" => "PackageDeclaration",
            // Kotlin parses `class`, `interface`, and `enum class` all as
            // `class_declaration`; `object`/`companion object` as object_declaration.
            "class_declaration" | "object_declaration" => "ClassDeclaration",
            "function_declaration" => "FunctionDeclaration",
            "property_declaration" => "VariableDeclaration",
            "if_expression" => "IfStatement",
            "return_expression" => "ReturnStatement",
            "comment" | "line_comment" | "multiline_comment" => return None,
            other => other,
        }
        .to_string();

        let name = name(node, src, ts_kind);
        let signature = signature(node, src, ts_kind);
        let lines = node_lines(node, src);
        let children = match ts_kind {
            // Surface members from the `class_body`.
            "class_declaration" | "object_declaration" => class_body_children(node, src),
            // Surface a function's body statements (function_body → block).
            "function_declaration" => function_body_children(node, src),
            _ => Vec::new(),
        };
        Some(Parsed {
            kind,
            name,
            signature,
            lines,
            children,
        })
    }

    fn class_body_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let mut c = node.walk();
        let Some(body) = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "class_body")
        else {
            return Vec::new();
        };
        let mut bc = body.walk();
        body.named_children(&mut bc)
            .filter_map(|n| map_node(n, src))
            .collect()
    }

    fn function_body_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let mut c = node.walk();
        let Some(fb) = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "function_body")
        else {
            return Vec::new();
        };
        // function_body wraps a `block`; descend into it.
        let mut fc = fb.walk();
        for inner in fb.named_children(&mut fc) {
            if inner.kind() == "block" {
                let mut ic = inner.walk();
                return inner
                    .named_children(&mut ic)
                    .filter_map(|n| map_node(n, src))
                    .collect();
            }
        }
        // Expression-body function (`fun f() = expr`) has no block.
        Vec::new()
    }

    fn name(node: Node, src: &[u8], ts_kind: &str) -> String {
        match ts_kind {
            "import" | "import_header" => qualified_identifier(node, src),
            "package_header" => qualified_identifier(node, src),
            "function_declaration" | "class_declaration" | "object_declaration" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| identifier_child(node, src)),
            // `val x = …` / `const val MAX = …` — the name lives in the
            // `variable_declaration` child's identifier.
            "property_declaration" => property_name(node, src),
            _ => snippet(node, src, 40),
        }
    }

    fn property_name(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let vd = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "variable_declaration");
        if let Some(vd) = vd {
            let mut vc = vd.walk();
            let id = vd
                .named_children(&mut vc)
                .find(|ch| ch.kind() == "identifier" || ch.kind() == "simple_identifier");
            if let Some(id) = id {
                return text(id, src).to_string();
            }
        }
        snippet(node, src, 40)
    }

    fn qualified_identifier(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| matches!(ch.kind(), "qualified_identifier" | "identifier"));
        found
            .map(|n| text(n, src).trim().to_string())
            .unwrap_or_else(|| snippet(node, src, 40))
    }

    fn identifier_child(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "identifier");
        found.map(|n| text(n, src).to_string()).unwrap_or_default()
    }

    fn signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
        if ts_kind != "function_declaration" {
            return None;
        }
        let mut c = node.walk();
        let params = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "function_value_parameters")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default();
        if params.is_empty() {
            None
        } else {
            Some(params)
        }
    }
}

mod swift {
    use super::{node_lines, snippet, text, Node, Parsed};

    pub fn map_node(node: Node, src: &[u8]) -> Option<Parsed> {
        let ts_kind = node.kind();
        let kind = match ts_kind {
            "import_declaration" => "ImportDeclaration",
            // Swift parses `class`, `struct`, and `enum` all as class_declaration.
            "class_declaration" => "ClassDeclaration",
            "protocol_declaration" => "InterfaceDeclaration",
            "function_declaration" | "protocol_function_declaration" | "init_declaration"
            | "deinit_declaration" => "FunctionDeclaration",
            "property_declaration" => "VariableDeclaration",
            "if_statement" => "IfStatement",
            "control_transfer_statement" => "ReturnStatement",
            "comment" | "multiline_comment" => return None,
            other => other,
        }
        .to_string();

        let name = name(node, src, ts_kind);
        let signature = signature(node, src, ts_kind);
        let lines = node_lines(node, src);
        let children = match ts_kind {
            // Surface members from the `class_body` / `protocol_body` / enum body.
            "class_declaration" => body_members(node, src, &["class_body", "enum_class_body"]),
            "protocol_declaration" => body_members(node, src, &["protocol_body"]),
            // Surface a function's body statements (function_body → statements).
            "function_declaration" | "init_declaration" | "deinit_declaration" => {
                function_body_children(node, src)
            }
            _ => Vec::new(),
        };
        Some(Parsed {
            kind,
            name,
            signature,
            lines,
            children,
        })
    }

    fn body_members(node: Node, src: &[u8], body_kinds: &[&str]) -> Vec<Parsed> {
        let mut c = node.walk();
        let Some(body) = node
            .named_children(&mut c)
            .find(|ch| body_kinds.contains(&ch.kind()))
        else {
            return Vec::new();
        };
        let mut bc = body.walk();
        body.named_children(&mut bc)
            .filter_map(|n| map_node(n, src))
            .collect()
    }

    fn function_body_children(node: Node, src: &[u8]) -> Vec<Parsed> {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        // `function_body` wraps a `statements` node.
        let mut bc = body.walk();
        for inner in body.named_children(&mut bc) {
            if inner.kind() == "statements" {
                let mut ic = inner.walk();
                return inner
                    .named_children(&mut ic)
                    .filter_map(|n| map_node(n, src))
                    .collect();
            }
        }
        Vec::new()
    }

    fn name(node: Node, src: &[u8], ts_kind: &str) -> String {
        match ts_kind {
            "import_declaration" => import_path(node, src),
            "class_declaration" | "protocol_declaration" => type_name(node, src),
            "function_declaration" | "protocol_function_declaration" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| simple_identifier(node, src)),
            "init_declaration" => "init".into(),
            "deinit_declaration" => "deinit".into(),
            "property_declaration" => node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_else(|| pattern_name(node, src)),
            _ => snippet(node, src, 40),
        }
    }

    fn import_path(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "identifier");
        found
            .map(|n| text(n, src).trim().to_string())
            .unwrap_or_else(|| snippet(node, src, 40))
    }

    fn type_name(node: Node, src: &[u8]) -> String {
        if let Some(n) = node.child_by_field_name("name") {
            return text(n, src).to_string();
        }
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "type_identifier");
        found.map(|n| text(n, src).to_string()).unwrap_or_default()
    }

    fn simple_identifier(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let found = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "simple_identifier");
        found.map(|n| text(n, src).to_string()).unwrap_or_default()
    }

    /// `let x = …` / `var x: T` — the binding's `pattern` holds the name.
    fn pattern_name(node: Node, src: &[u8]) -> String {
        let mut c = node.walk();
        let pat = node
            .named_children(&mut c)
            .find(|ch| ch.kind() == "pattern");
        if let Some(pat) = pat {
            return text(pat, src).to_string();
        }
        snippet(node, src, 40)
    }

    fn signature(node: Node, src: &[u8], ts_kind: &str) -> Option<String> {
        if !matches!(
            ts_kind,
            "function_declaration" | "protocol_function_declaration"
        ) {
            return None;
        }
        // Swift parameters are loose `parameter` children between `(` and `)`.
        let mut c = node.walk();
        let params: Vec<String> = node
            .named_children(&mut c)
            .filter(|ch| ch.kind() == "parameter")
            .map(|n| text(n, src).to_string())
            .collect();
        let ret = node
            .child_by_field_name("return_type")
            .map(|n| format!(" -> {}", text(n, src)))
            .unwrap_or_default();
        if params.is_empty() && ret.is_empty() {
            None
        } else {
            Some(format!("({}){}", params.join(", "), ret))
        }
    }
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
        // Core-four structural-drift languages.
        assert_eq!(Lang::from_path("a.rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_path("a.go"), Some(Lang::Go));
        assert_eq!(Lang::from_path("a.py"), Some(Lang::Python));
        assert_eq!(Lang::from_path("a.pyi"), Some(Lang::Python));
        assert_eq!(Lang::from_path("Main.java"), Some(Lang::Java));
        // Stretch structural-drift languages.
        assert_eq!(Lang::from_path("Service.cs"), Some(Lang::CSharp));
        assert_eq!(Lang::from_path("Main.kt"), Some(Lang::Kotlin));
        assert_eq!(Lang::from_path("build.kts"), Some(Lang::Kotlin));
        assert_eq!(Lang::from_path("App.swift"), Some(Lang::Swift));
        assert_eq!(Lang::from_path("a.d.ts"), None);
        assert_eq!(Lang::from_path("package.json"), None);
        assert_eq!(Lang::from_path("README.md"), None);
    }

    #[test]
    fn labels_and_families_for_every_lang() {
        for (lang, label, fam) in [
            (Lang::Ts, "TypeScript", Family::JsTs),
            (Lang::Tsx, "TSX", Family::JsTs),
            (Lang::Js, "JavaScript", Family::JsTs),
            (Lang::Jsx, "JSX", Family::JsTs),
            (Lang::Rust, "Rust", Family::Rust),
            (Lang::Go, "Go", Family::Go),
            (Lang::Python, "Python", Family::Python),
            (Lang::Java, "Java", Family::Java),
            (Lang::CSharp, "C#", Family::CSharp),
            (Lang::Kotlin, "Kotlin", Family::Kotlin),
            (Lang::Swift, "Swift", Family::Swift),
        ] {
            assert_eq!(lang.label(), label);
            assert_eq!(lang.family(), fam);
        }
    }

    // ---- Rust ----
    const RUST_SRC: &str = r#"use std::fs::File;

const MAX_RETRIES: u32 = 3;

struct Point {
    x: i32,
}

trait Shape {
    fn area(&self) -> i32;
}

fn process(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return false;
    }
    validate(trimmed)
}

impl Point {
    fn area(&self) -> i32 {
        self.x
    }
}
"#;

    #[test]
    fn rust_top_level_skeleton() {
        let nodes = parse_file(RUST_SRC, Lang::Rust);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("ImportDeclaration", "std::fs::File"),
                ("VariableDeclaration", "MAX_RETRIES"),
                ("ClassDeclaration", "Point"),
                ("InterfaceDeclaration", "Shape"),
                ("FunctionDeclaration", "process"),
                ("ImplBlock", "Point"),
            ]
        );
        let process = &nodes[4];
        assert_eq!(process.signature.as_deref(), Some("(input: &str) -> bool"));
    }

    #[test]
    fn rust_function_body_children() {
        let nodes = parse_file(RUST_SRC, Lang::Rust);
        let process = &nodes[4];
        let child_kinds: Vec<&str> = process.children.iter().map(|c| c.kind.as_str()).collect();
        // `let trimmed = …`, `if … { … }` (an expression_statement), and the
        // tail expression `validate(trimmed)`.
        assert_eq!(child_kinds[0], "VariableDeclaration");
        assert_eq!(process.children[0].name, "trimmed");
        assert!(child_kinds.contains(&"ExpressionStatement"));
    }

    // ---- Go ----
    const GO_SRC: &str = r#"package main

import "fmt"

const Pi = 3

type Point struct {
    X int
}

func process(input string) bool {
    trimmed := trim(input)
    if trimmed == "" {
        return false
    }
    return validate(trimmed)
}

func (p Point) Area() int {
    return p.X
}
"#;

    #[test]
    fn go_top_level_skeleton() {
        let nodes = parse_file(GO_SRC, Lang::Go);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("PackageDeclaration", "package main"),
                ("ImportDeclaration", "fmt"),
                ("VariableDeclaration", "Pi"),
                ("ClassDeclaration", "Point"),
                ("FunctionDeclaration", "process"),
                ("FunctionDeclaration", "Area"),
            ]
        );
    }

    #[test]
    fn go_function_body_children_descend_through_statement_list() {
        let nodes = parse_file(GO_SRC, Lang::Go);
        let process = &nodes[4];
        let child_kinds: Vec<&str> = process.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(
            child_kinds,
            vec!["VariableDeclaration", "IfStatement", "ReturnStatement"]
        );
        assert_eq!(process.children[0].name, "trimmed");
    }

    // ---- Python ----
    const PY_SRC: &str = r#"import os
from sys import path

MAX_RETRIES = 3

def process(input):
    trimmed = input.strip()
    if not trimmed:
        return False
    return validate(trimmed)

class Worker:
    def run(self):
        return 1

@app.route("/x")
def handler():
    return ok()
"#;

    #[test]
    fn python_top_level_skeleton() {
        let nodes = parse_file(PY_SRC, Lang::Python);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("ImportDeclaration", "os"),
                ("ImportDeclaration", "sys"),
                ("VariableDeclaration", "MAX_RETRIES"),
                ("FunctionDeclaration", "process"),
                ("ClassDeclaration", "Worker"),
                // A decorated def unwraps to the function it decorates.
                ("FunctionDeclaration", "handler"),
            ]
        );
    }

    #[test]
    fn python_function_body_children() {
        let nodes = parse_file(PY_SRC, Lang::Python);
        let process = &nodes[3];
        let child_kinds: Vec<&str> = process.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(
            child_kinds,
            vec!["VariableDeclaration", "IfStatement", "ReturnStatement"]
        );
        assert_eq!(process.children[0].name, "trimmed");
    }

    // ---- Java ----
    const JAVA_SRC: &str = r#"package com.example;

import java.util.List;

class Service {
    int retries = 3;

    boolean process(String input) {
        String trimmed = input.trim();
        if (trimmed.isEmpty()) {
            return false;
        }
        return validate(trimmed);
    }
}

interface Shape {}
"#;

    #[test]
    fn java_top_level_skeleton() {
        let nodes = parse_file(JAVA_SRC, Lang::Java);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("PackageDeclaration", "com.example"),
                ("ImportDeclaration", "java.util.List"),
                ("ClassDeclaration", "Service"),
                ("InterfaceDeclaration", "Shape"),
            ]
        );
    }

    #[test]
    fn java_class_members_and_method_body_surface() {
        let nodes = parse_file(JAVA_SRC, Lang::Java);
        let service = &nodes[2];
        let members: Vec<(&str, &str)> = service
            .children
            .iter()
            .map(|c| (c.kind.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(
            members,
            vec![
                ("VariableDeclaration", "retries"),
                ("FunctionDeclaration", "process"),
            ]
        );
        let process = &service.children[1];
        let stmt_kinds: Vec<&str> = process.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(
            stmt_kinds,
            vec!["VariableDeclaration", "IfStatement", "ReturnStatement"]
        );
        assert_eq!(process.children[0].name, "trimmed");
    }

    // ---- C# ----
    const CS_SRC: &str = r#"using System;
using System.Collections.Generic;

namespace Demo;

class Service {
    private int retries = 3;

    public bool Process(string input) {
        string trimmed = input.Trim();
        if (string.IsNullOrEmpty(trimmed)) {
            return false;
        }
        return Validate(trimmed);
    }
}

interface IShape {
    int Area();
}

enum Color { Red, Green }
"#;

    #[test]
    fn csharp_top_level_skeleton() {
        let nodes = parse_file(CS_SRC, Lang::CSharp);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("ImportDeclaration", "System"),
                ("ImportDeclaration", "System.Collections.Generic"),
                ("PackageDeclaration", "Demo"),
                ("ClassDeclaration", "Service"),
                ("InterfaceDeclaration", "IShape"),
                ("ClassDeclaration", "Color"),
            ]
        );
    }

    #[test]
    fn csharp_class_members_and_method_body_surface() {
        let nodes = parse_file(CS_SRC, Lang::CSharp);
        let service = &nodes[3];
        assert_eq!(service.name, "Service");
        let members: Vec<(&str, &str)> = service
            .children
            .iter()
            .map(|c| (c.kind.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(
            members,
            vec![
                ("VariableDeclaration", "retries"),
                ("FunctionDeclaration", "Process"),
            ]
        );
        let process = &service.children[1];
        assert_eq!(process.signature.as_deref(), Some("(string input): bool"));
        let stmt_kinds: Vec<&str> = process.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(
            stmt_kinds,
            vec!["VariableDeclaration", "IfStatement", "ReturnStatement"]
        );
        assert_eq!(process.children[0].name, "trimmed");
    }

    // ---- Kotlin ----
    const KT_SRC: &str = r#"package com.example

import java.util.List

const val MAX = 3

fun process(input: String): Boolean {
    val trimmed = input.trim()
    if (trimmed.isEmpty()) {
        return false
    }
    return validate(trimmed)
}

class Service {
    fun run(): Int {
        return 1
    }
}

interface Shape {
    fun area(): Int
}
"#;

    #[test]
    fn kotlin_top_level_skeleton() {
        let nodes = parse_file(KT_SRC, Lang::Kotlin);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("PackageDeclaration", "com.example"),
                ("ImportDeclaration", "java.util.List"),
                ("VariableDeclaration", "MAX"),
                ("FunctionDeclaration", "process"),
                ("ClassDeclaration", "Service"),
                // Kotlin parses `interface` as a class_declaration; it groups
                // under ClassDeclaration (honest structural mapping).
                ("ClassDeclaration", "Shape"),
            ]
        );
        let process = &nodes[3];
        assert_eq!(process.signature.as_deref(), Some("(input: String)"));
    }

    #[test]
    fn kotlin_function_body_children() {
        let nodes = parse_file(KT_SRC, Lang::Kotlin);
        let process = &nodes[3];
        let child_kinds: Vec<&str> = process.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(
            child_kinds,
            vec!["VariableDeclaration", "IfStatement", "ReturnStatement"]
        );
        assert_eq!(process.children[0].name, "trimmed");
        // Class members surface too.
        let service = &nodes[4];
        let members: Vec<(&str, &str)> = service
            .children
            .iter()
            .map(|c| (c.kind.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(members, vec![("FunctionDeclaration", "run")]);
    }

    // ---- Swift ----
    const SWIFT_SRC: &str = r#"import Foundation

let maxRetries = 3

func process(input: String) -> Bool {
    let trimmed = input.trimmed()
    if trimmed.isEmpty {
        return false
    }
    return validate(trimmed)
}

class Service {
    var retries = 3
    func run() -> Int {
        return 1
    }
}

struct Point {
    var x: Int
}

protocol Shape {
    func area() -> Int
}
"#;

    #[test]
    fn swift_top_level_skeleton() {
        let nodes = parse_file(SWIFT_SRC, Lang::Swift);
        let kinds: Vec<(&str, &str)> = nodes
            .iter()
            .map(|n| (n.kind.as_str(), n.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("ImportDeclaration", "Foundation"),
                ("VariableDeclaration", "maxRetries"),
                ("FunctionDeclaration", "process"),
                ("ClassDeclaration", "Service"),
                // Swift parses `struct` as a class_declaration.
                ("ClassDeclaration", "Point"),
                ("InterfaceDeclaration", "Shape"),
            ]
        );
        let process = &nodes[2];
        assert_eq!(process.signature.as_deref(), Some("(input: String) -> Bool"));
    }

    #[test]
    fn swift_function_body_and_class_members_surface() {
        let nodes = parse_file(SWIFT_SRC, Lang::Swift);
        let process = &nodes[2];
        let child_kinds: Vec<&str> = process.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(
            child_kinds,
            vec!["VariableDeclaration", "IfStatement", "ReturnStatement"]
        );
        assert_eq!(process.children[0].name, "trimmed");
        let service = &nodes[3];
        let members: Vec<(&str, &str)> = service
            .children
            .iter()
            .map(|c| (c.kind.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(
            members,
            vec![
                ("VariableDeclaration", "retries"),
                ("FunctionDeclaration", "run"),
            ]
        );
    }

    #[test]
    fn new_languages_tolerate_empty_and_garbage() {
        for lang in [
            Lang::Rust,
            Lang::Go,
            Lang::Python,
            Lang::Java,
            Lang::CSharp,
            Lang::Kotlin,
            Lang::Swift,
        ] {
            assert!(parse_file("", lang).is_empty());
            let _ = parse_file("@@@ ?? not valid {{{ ;;;", lang);
        }
    }
}
