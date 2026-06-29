//! Language registry + import extraction via tree-sitter.
//!
//! This is the structural backend. A file's language is inferred from its
//! extension; each language knows how to extract its imports as `(path, line)`
//! candidates in slash form, which the structural matcher then globs against a
//! rule's `from`/`to`. Adding a language = a grammar + an extractor here, with
//! no changes to the matcher or the rest of the engine.
//!
//! Imports are returned as slash paths so one rule (`to: "**/notifications/**"`)
//! works across languages: Python `a.b.c` -> `a/b/c`, Go `"a/b/c"` stays as is.

use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy)]
pub enum Lang {
    Python,
    Go,
}

/// Infer a language from a file path, or `None` for unsupported extensions
/// (those files are skipped by the structural matcher).
pub fn lang_for_path(path: &str) -> Option<Lang> {
    let p = path.strip_prefix("./").unwrap_or(path);
    if p.ends_with(".py") {
        Some(Lang::Python)
    } else if p.ends_with(".go") {
        Some(Lang::Go)
    } else {
        None
    }
}

fn dots_to_slash(s: &str) -> String {
    s.replace('.', "/")
}

/// Parse `src` and return its imports as `(slash_path, line_1_based)`.
///
/// Returns `None` if the file does not parse cleanly (tree has errors) — the
/// engine then skips it rather than enforcing structural rules on broken code,
/// matching the original fail-open behavior.
pub fn import_candidates(src: &str, lang: Lang) -> Option<Vec<(String, usize)>> {
    let language = match lang {
        Lang::Python => tree_sitter_python::LANGUAGE,
        Lang::Go => tree_sitter_go::LANGUAGE,
    };
    let mut parser = Parser::new();
    parser.set_language(&language.into()).ok()?;
    let tree = parser.parse(src, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let bytes = src.as_bytes();
    let mut out: Vec<(String, usize)> = Vec::new();
    match lang {
        Lang::Python => walk_python(root, bytes, &mut out),
        Lang::Go => walk_go(root, bytes, &mut out),
    }
    Some(out)
}

fn named_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn text(node: Node, src: &[u8]) -> Option<String> {
    node.utf8_text(src).ok().map(|s| s.to_string())
}

fn walk_python(node: Node, src: &[u8], out: &mut Vec<(String, usize)>) {
    match node.kind() {
        "import_statement" => {
            let line = node.start_position().row + 1;
            for child in named_children(node) {
                match child.kind() {
                    "dotted_name" => {
                        if let Some(t) = text(child, src) {
                            out.push((dots_to_slash(&t), line));
                        }
                    }
                    "aliased_import" => {
                        if let Some(name) = child.child_by_field_name("name")
                            && let Some(t) = text(name, src)
                        {
                            out.push((dots_to_slash(&t), line));
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        "import_from_statement" => {
            let line = node.start_position().row + 1;
            let module = node.child_by_field_name("module_name");
            // Relative imports (`from . import x`) are out of scope — skip.
            if let Some(m) = module
                && m.kind() != "relative_import"
                && let Some(base) = text(m, src).map(|t| dots_to_slash(&t))
            {
                out.push((base.clone(), line));
                // imported names: `from base import a, b as c`
                let mut cursor = node.walk();
                for child in node.children_by_field_name("name", &mut cursor) {
                    let name = match child.kind() {
                        "dotted_name" => text(child, src),
                        "aliased_import" => {
                            child.child_by_field_name("name").and_then(|n| text(n, src))
                        }
                        _ => None,
                    };
                    if let Some(name) = name {
                        out.push((format!("{base}/{}", dots_to_slash(&name)), line));
                    }
                }
                // `from base import *`
                for child in named_children(node) {
                    if child.kind() == "wildcard_import" {
                        out.push((format!("{base}/*"), line));
                    }
                }
            }
            return;
        }
        _ => {}
    }
    for child in named_children(node) {
        walk_python(child, src, out);
    }
}

fn walk_go(node: Node, src: &[u8], out: &mut Vec<(String, usize)>) {
    if node.kind() == "import_spec" {
        let line = node.start_position().row + 1;
        if let Some(path) = node.child_by_field_name("path")
            && let Some(t) = text(path, src)
        {
            let trimmed = t.trim_matches('"').trim_matches('`');
            out.push((trimmed.to_string(), line));
        }
        return;
    }
    for child in named_children(node) {
        walk_go(child, src, out);
    }
}
