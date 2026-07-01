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

/// One import found in a file.
#[derive(Debug, Clone)]
pub struct ImportRef {
    /// The imported module as a slash path (`a.b.c` -> `a/b/c`).
    pub path: String,
    /// 1-based line of the import statement (from tree-sitter).
    pub line: usize,
    /// The offending source line — the import statement's own text, straight
    /// from the tree-sitter node, so it shares the node's line (single source).
    pub snippet: String,
}

/// Parse `src` and return its imports.
///
/// Returns `None` if the file does not parse cleanly (tree has errors) — the
/// engine then skips it rather than enforcing structural rules on broken code,
/// matching the original fail-open behavior.
pub fn import_candidates(src: &str, lang: Lang) -> Option<Vec<ImportRef>> {
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
    let mut out: Vec<ImportRef> = Vec::new();
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

/// The node's first source line, trimmed — a display-ready snippet from
/// tree-sitter itself (no re-slicing the file by a possibly-divergent index).
fn line_snippet(node: Node, src: &[u8]) -> String {
    node.utf8_text(src)
        .ok()
        .and_then(|t| t.lines().next())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn walk_python(node: Node, src: &[u8], out: &mut Vec<ImportRef>) {
    match node.kind() {
        "import_statement" => {
            let line = node.start_position().row + 1;
            let snippet = line_snippet(node, src);
            for child in named_children(node) {
                match child.kind() {
                    "dotted_name" => {
                        if let Some(t) = text(child, src) {
                            out.push(ImportRef {
                                path: dots_to_slash(&t),
                                line,
                                snippet: snippet.clone(),
                            });
                        }
                    }
                    "aliased_import" => {
                        if let Some(name) = child.child_by_field_name("name")
                            && let Some(t) = text(name, src)
                        {
                            out.push(ImportRef {
                                path: dots_to_slash(&t),
                                line,
                                snippet: snippet.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        "import_from_statement" => {
            let line = node.start_position().row + 1;
            let snippet = line_snippet(node, src);
            let module = node.child_by_field_name("module_name");
            // Relative imports (`from . import x`) are out of scope — skip.
            if let Some(m) = module
                && m.kind() != "relative_import"
                && let Some(base) = text(m, src).map(|t| dots_to_slash(&t))
            {
                out.push(ImportRef {
                    path: base.clone(),
                    line,
                    snippet: snippet.clone(),
                });
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
                        out.push(ImportRef {
                            path: format!("{base}/{}", dots_to_slash(&name)),
                            line,
                            snippet: snippet.clone(),
                        });
                    }
                }
                // `from base import *`
                for child in named_children(node) {
                    if child.kind() == "wildcard_import" {
                        out.push(ImportRef {
                            path: format!("{base}/*"),
                            line,
                            snippet: snippet.clone(),
                        });
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

fn walk_go(node: Node, src: &[u8], out: &mut Vec<ImportRef>) {
    if node.kind() == "import_spec" {
        let line = node.start_position().row + 1;
        if let Some(path) = node.child_by_field_name("path")
            && let Some(t) = text(path, src)
        {
            let trimmed = t.trim_matches('"').trim_matches('`');
            out.push(ImportRef {
                path: trimmed.to_string(),
                line,
                snippet: line_snippet(node, src),
            });
        }
        return;
    }
    for child in named_children(node) {
        walk_go(child, src, out);
    }
}
