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

use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Python,
    Go,
    Rust,
}

/// The tree-sitter grammar for a language. Named here once so both the import
/// walker and the query engine share the same source of truth.
fn ts_language(lang: Lang) -> tree_sitter::Language {
    let raw = match lang {
        Lang::Python => tree_sitter_python::LANGUAGE,
        Lang::Go => tree_sitter_go::LANGUAGE,
        Lang::Rust => tree_sitter_rust::LANGUAGE,
    };
    raw.into()
}

/// Infer a language from a file path, or `None` for unsupported extensions
/// (those files are skipped by the structural and query matchers).
pub fn lang_for_path(path: &str) -> Option<Lang> {
    let p = path.strip_prefix("./").unwrap_or(path);
    if p.ends_with(".py") {
        Some(Lang::Python)
    } else if p.ends_with(".go") {
        Some(Lang::Go)
    } else if p.ends_with(".rs") {
        Some(Lang::Rust)
    } else {
        None
    }
}

/// Resolve the `language:` field of a `query` rule to a `Lang`. This is the
/// authoritative list of names a rule author may write.
pub fn lang_by_name(name: &str) -> Option<Lang> {
    match name {
        "python" => Some(Lang::Python),
        "go" => Some(Lang::Go),
        "rust" => Some(Lang::Rust),
        _ => None,
    }
}

/// The set of language names accepted in a `query` rule's `language:` field,
/// for validation error messages.
pub const QUERY_LANGUAGES: [&str; 3] = ["python", "go", "rust"];

fn dots_to_slash(s: &str) -> String {
    s.replace('.', "/")
}

/// One import found in a file.
#[derive(Debug, Clone)]
pub struct ImportRef {
    /// The imported module as a slash path (`a.b.c` -> `a/b/c`).
    pub path: String,
    /// 1-based line of *this candidate's* node (an imported name in a multi-line
    /// import points at its own line, not the `from` line). See `locate`.
    pub line: usize,
    /// The source line at `line`, trimmed — the offending line to show. Line and
    /// snippet share the candidate node's row (see `locate`), so they agree.
    pub snippet: String,
}

/// Parse `src` and return its imports.
///
/// Returns `None` if the file does not parse cleanly (tree has errors) — the
/// engine then skips it rather than enforcing structural rules on broken code,
/// matching the original fail-open behavior.
pub fn import_candidates(src: &str, lang: Lang) -> Option<Vec<ImportRef>> {
    let mut parser = Parser::new();
    parser.set_language(&ts_language(lang)).ok()?;
    let tree = parser.parse(src, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let mut out: Vec<ImportRef> = Vec::new();
    match lang {
        Lang::Python => walk_python(root, src, &mut out),
        Lang::Go => walk_go(root, src, &mut out),
        // No import walker for Rust yet — structural import rules simply find
        // nothing here. Rust is wired for the `query` matcher, which needs no
        // per-language walker (see `run_query`).
        Lang::Rust => {}
    }
    Some(out)
}

/// Compile a tree-sitter query string against `lang`'s grammar. Used at
/// validate time (so a malformed `.scm` fails when rules load, not silently at
/// runtime) and by the query matcher. The error is the tree-sitter message.
pub fn compile_query(lang: Lang, query: &str) -> Result<Query, String> {
    Query::new(&ts_language(lang), query).map_err(|e| e.to_string())
}

/// Run a compiled query over `src` and return one hit per capture, located at
/// the captured node (1-based line + trimmed source line, via `locate`).
///
/// Returns `None` if the file does not parse cleanly (tree has errors) — the
/// query matcher then skips it, matching the structural matcher's fail-open
/// behavior. Hits are deduped by line so one source line yields at most one
/// violation, mirroring the structural matcher.
pub fn run_query(src: &str, lang: Lang, query: &Query) -> Option<Vec<(usize, String)>> {
    let mut parser = Parser::new();
    parser.set_language(&ts_language(lang)).ok()?;
    let tree = parser.parse(src, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let mut out: Vec<(usize, String)> = Vec::new();
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut cursor = QueryCursor::new();
    // `matches` applies the query's text predicates (`#eq?`, `#match?`, …)
    // automatically as it streams, so a query like `(#eq? @m "unwrap")` only
    // yields matches whose captured text satisfies the predicate.
    let mut it = cursor.matches(query, root, src.as_bytes());
    while let Some(m) = it.next() {
        for cap in m.captures {
            let (line, snippet) = locate(cap.node, src);
            if seen.insert(line) {
                out.push((line, snippet));
            }
        }
    }
    out.sort_by_key(|(line, _)| *line);
    Some(out)
}

fn named_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn text(node: Node, src: &str) -> Option<String> {
    node.utf8_text(src.as_bytes()).ok().map(|s| s.to_string())
}

/// A node's 1-based line plus the source line it sits on (trimmed). Both derive
/// from the node's own row, so they always agree — and because each import
/// candidate is located at *its* node, a name inside a parenthesized multi-line
/// import points at that name's line, not the `from` line.
fn locate(node: Node, src: &str) -> (usize, String) {
    let row = node.start_position().row;
    let snippet = src.lines().nth(row).unwrap_or("").trim().to_string();
    (row + 1, snippet)
}

fn push_import(out: &mut Vec<ImportRef>, path: String, at: Node, src: &str) {
    let (line, snippet) = locate(at, src);
    out.push(ImportRef {
        path,
        line,
        snippet,
    });
}

fn walk_python(node: Node, src: &str, out: &mut Vec<ImportRef>) {
    match node.kind() {
        "import_statement" => {
            for child in named_children(node) {
                match child.kind() {
                    "dotted_name" => {
                        if let Some(t) = text(child, src) {
                            push_import(out, dots_to_slash(&t), child, src);
                        }
                    }
                    "aliased_import" => {
                        if let Some(name) = child.child_by_field_name("name")
                            && let Some(t) = text(name, src)
                        {
                            push_import(out, dots_to_slash(&t), name, src);
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        "import_from_statement" => {
            let module = node.child_by_field_name("module_name");
            // Relative imports (`from . import x`) are out of scope — skip.
            if let Some(m) = module
                && m.kind() != "relative_import"
                && let Some(base) = text(m, src).map(|t| dots_to_slash(&t))
            {
                // Base candidate, located at the module name.
                push_import(out, base.clone(), m, src);
                // Imported names, each located at its OWN node — so a forbidden
                // name in a multi-line `from a import ( … )` points at that name.
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
                        push_import(out, format!("{base}/{}", dots_to_slash(&name)), child, src);
                    }
                }
                // `from base import *`
                for child in named_children(node) {
                    if child.kind() == "wildcard_import" {
                        push_import(out, format!("{base}/*"), child, src);
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

fn walk_go(node: Node, src: &str, out: &mut Vec<ImportRef>) {
    if node.kind() == "import_spec" {
        if let Some(path) = node.child_by_field_name("path")
            && let Some(t) = text(path, src)
        {
            let trimmed = t.trim_matches('"').trim_matches('`');
            push_import(out, trimmed.to_string(), node, src);
        }
        return;
    }
    for child in named_children(node) {
        walk_go(child, src, out);
    }
}
