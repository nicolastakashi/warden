//! File-path glob matching with Python `fnmatch` semantics.
//!
//! The engine matches a rule's `paths` and a structural rule's `from`/`to`
//! globs against paths the same way the original Python did: `fnmatch`, where
//! `*` and `?` cross `/` (there is no special `**` token — `*` already spans
//! separators). We translate the glob to a regex exactly as CPython's
//! `fnmatch.translate` does so behavior is identical.

use regex::Regex;

fn translate(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    // A *leading* `**/` matches zero or more leading path segments
    // (gitignore-style globstar), so `**/src/**` matches a top-level `src/…` as
    // well as a nested `a/src/…`. Without this, the `**/` still needs a literal
    // `/` before it (fnmatch's `*` crosses `/` but doesn't conjure the
    // separator), which is the documented glob footgun. Only a leading `**/` is
    // special-cased; `*`/`**` elsewhere keep fnmatch semantics.
    if chars.starts_with(&['*', '*', '/']) {
        out.push_str("(?:.*/)?");
        i = 3;
    }
    while i < n {
        let c = chars[i];
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '[' => {
                // Find the matching ']'.
                let mut j = i + 1;
                if j < n && chars[j] == '!' {
                    j += 1;
                }
                if j < n && chars[j] == ']' {
                    j += 1;
                }
                while j < n && chars[j] != ']' {
                    j += 1;
                }
                if j >= n {
                    // No closing bracket — treat '[' as a literal.
                    out.push_str("\\[");
                } else {
                    let inner: String = chars[i + 1..j].iter().collect();
                    let inner = inner.replace('\\', "\\\\");
                    if let Some(rest) = inner.strip_prefix('!') {
                        out.push_str(&format!("[^{rest}]"));
                    } else {
                        out.push_str(&format!("[{inner}]"));
                    }
                    i = j;
                }
            }
            _ => out.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    // (?s): '.' matches newline, like CPython's `(?s:...)`. Anchored start/end.
    format!(r"(?s)\A{out}\z")
}

/// True if `name` matches the glob `pattern` (fnmatch semantics).
pub fn fnmatch(name: &str, pattern: &str) -> bool {
    match Regex::new(&translate(pattern)) {
        Ok(re) => re.is_match(name),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::fnmatch;

    #[test]
    fn star_crosses_slash() {
        assert!(fnmatch("src/billing/charge.py", "**/billing/**"));
        assert!(fnmatch("src/billing/charge.py", "src/billing/**"));
        assert!(fnmatch("lib/a.py", "lib/*.py"));
        assert!(!fnmatch("src/api/h.py", "src/billing/**"));
    }

    #[test]
    fn module_paths() {
        assert!(fnmatch("src/notifications/email", "**/notifications/**"));
        assert!(!fnmatch("src/notifications", "**/notifications/**"));
    }

    #[test]
    fn leading_globstar_matches_top_level_and_nested() {
        // R6: a leading `**/` matches zero-or-more leading segments, so one glob
        // covers both the repo-root case and the nested case — no more
        // `src/**` + `**/src/**` duplication.
        assert!(fnmatch("src/main.rs", "**/src/**")); // top-level (was the footgun)
        assert!(fnmatch("a/b/src/main.rs", "**/src/**")); // nested
        assert!(fnmatch("foo/bar.py", "**/foo/**")); // structural: top-level package
        assert!(fnmatch("app/foo/bar.py", "**/foo/**")); // nested package
        assert!(!fnmatch("other/bar.py", "**/foo/**")); // still ignores unrelated
        // A leading `**/` before a filename matches it at the root, too.
        assert!(fnmatch("ci_gate.rs", "**/ci_gate.rs"));
        assert!(fnmatch("src/ci_gate.rs", "**/ci_gate.rs"));
    }
}
