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
}
