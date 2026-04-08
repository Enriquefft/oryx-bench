//! `process-record-user-collision` — Tier 2 file defines `process_record_user`
//! directly, colliding with the generator's emitted one.

use crate::lint::{Issue, LintContext, LintRule, Severity};

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "process-record-user-collision"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn description(&self) -> &'static str {
        "A Tier 2 file (`*.zig` or vendored `*.c`) defines `process_record_user` directly, colliding with the generator's auto-emitted `process_record_user`."
    }
    fn why_bad(&self) -> &'static str {
        "The link step fails with a duplicate symbol error."
    }
    fn fix_example(&self) -> &'static str {
        "Rename the Tier 2 function to `process_record_user_overlay`. The generated `process_record_user` dispatches to `_overlay` after handling its own concerns. Same applies to `matrix_scan_user`, `keyboard_post_init_user`, etc."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        let overlay = ctx.project.overlay_dir();
        if !overlay.exists() {
            return out;
        }
        // Walk overlay/. Walkdir errors (broken symlinks, permission
        // denied) and read errors are surfaced as info-severity issues
        // so the user notices them — silently dropping unreadable
        // files would let a real collision slip through unnoticed.
        for entry in walkdir::WalkDir::new(&overlay) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    out.push(Issue {
                        rule_id: self.id().to_string(),
                        severity: Severity::Info,
                        message: format!("could not walk overlay/: {e}"),
                        layer: None,
                        position_index: None,
                    });
                    continue;
                }
            };
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                continue;
            };
            if !matches!(ext, "c" | "zig") {
                continue;
            }
            let contents = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    out.push(Issue {
                        rule_id: self.id().to_string(),
                        severity: Severity::Info,
                        message: format!("could not read {}: {e}", path.display()),
                        layer: None,
                        position_index: None,
                    });
                    continue;
                }
            };
            if file_defines_process_record_user(&contents) {
                out.push(Issue {
                    rule_id: self.id().to_string(),
                    severity: self.severity(),
                    message: format!(
                        "{} defines process_record_user — rename to process_record_user_overlay",
                        path.display()
                    ),
                    layer: None,
                    position_index: None,
                });
            }
        }
        out
    }
}

/// True if the file contains an actual function *definition* (not a call,
/// declaration, or comment) of `process_record_user`. The check requires
/// the function name immediately followed by `(` and then the body's
/// opening `{` on the same or following non-comment line.
fn file_defines_process_record_user(contents: &str) -> bool {
    // Strip block comments first.
    let no_block_comments = strip_block_comments(contents);
    let mut state = State::Scan;
    for raw_line in no_block_comments.lines() {
        let line = raw_line.split("//").next().unwrap_or(raw_line).trim_start();
        if line.is_empty() {
            continue;
        }
        match state {
            State::Scan => {
                // Look for `process_record_user(`. Skip:
                //  - extern declarations
                //  - lines containing `process_record_user_overlay`
                //  - call sites that don't end with `{` and don't precede a brace
                if line.starts_with("extern") {
                    continue;
                }
                if line.contains("process_record_user_overlay") {
                    continue;
                }
                let Some(paren_idx) = line.find("process_record_user(") else {
                    continue;
                };
                // Must be preceded by a return type token (whitespace,
                // not `.`, `=`, `(`, or `>`) — call-site context.
                if paren_idx > 0 {
                    let prev = line.as_bytes()[paren_idx - 1];
                    if !prev.is_ascii_whitespace() {
                        continue;
                    }
                }
                // Distinguish a function definition from a forward
                // declaration: a definition has `{` somewhere after the
                // closing `)`; a declaration has `;`. We look for whichever
                // appears first.
                let after = &line[paren_idx..];
                let first_brace = after.find('{');
                let first_semi = after.find(';');
                match (first_brace, first_semi) {
                    (Some(b), Some(s)) if b < s => return true,
                    (Some(_), None) => return true,
                    (None, Some(_)) => continue,
                    (None, None) => {
                        // Header continues on the next line. Wait for `{`
                        // or `;` on subsequent lines.
                        state = State::AwaitBrace;
                    }
                    (Some(_), Some(_)) => continue, // declaration before brace — call site or forward decl
                }
            }
            State::AwaitBrace => {
                if line.contains('{') {
                    return true;
                }
                if line.contains(';') {
                    state = State::Scan;
                }
            }
        }
    }
    false
}

enum State {
    Scan,
    AwaitBrace,
}

fn strip_block_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Skip until matching */
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2; // skip */
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_basic_definition() {
        let src = "bool process_record_user(uint16_t kc, keyrecord_t *r) { return true; }\n";
        assert!(file_defines_process_record_user(src));
    }

    #[test]
    fn detects_definition_with_brace_on_next_line() {
        let src = "bool process_record_user(uint16_t kc, keyrecord_t *r)\n{ return true; }\n";
        assert!(file_defines_process_record_user(src));
    }

    #[test]
    fn ignores_extern_declaration() {
        let src = "extern bool process_record_user(uint16_t kc, keyrecord_t *r);\n";
        assert!(!file_defines_process_record_user(src));
    }

    #[test]
    fn ignores_overlay_variant() {
        let src =
            "bool process_record_user_overlay(uint16_t kc, keyrecord_t *r) { return true; }\n";
        assert!(!file_defines_process_record_user(src));
    }

    #[test]
    fn ignores_call_site() {
        let src = "void helper() { process_record_user(KC_A, &record); }\n";
        assert!(!file_defines_process_record_user(src));
    }

    #[test]
    fn ignores_line_comment() {
        let src = "// process_record_user(KC_A, &record);\n";
        assert!(!file_defines_process_record_user(src));
    }

    #[test]
    fn ignores_block_comment() {
        let src = "/* bool process_record_user(uint16_t kc, keyrecord_t *r) { return true; } */\n";
        assert!(!file_defines_process_record_user(src));
    }

    #[test]
    fn detects_zig_definition() {
        // QMK's `process_record_user` can be defined from a Zig file
        // via the C ABI. The scanner treats `.zig` files with the
        // same per-line heuristic as C, so the detection must hold.
        let src = "export fn process_record_user(keycode: u16, record: *anyopaque) callconv(.C) bool {\n    return true;\n}\n";
        assert!(file_defines_process_record_user(src));
    }

    #[test]
    fn detects_zig_definition_with_brace_on_next_line() {
        let src = "export fn process_record_user(\n    keycode: u16,\n    record: *anyopaque,\n) bool {\n    return true;\n}\n";
        assert!(file_defines_process_record_user(src));
    }

    #[test]
    fn ignores_zig_extern_declaration() {
        // Zig's `extern` declarations mirror C's — they're forward
        // decls, not definitions.
        let src = "extern fn process_record_user(keycode: u16, record: *anyopaque) bool;\n";
        assert!(!file_defines_process_record_user(src));
    }
}
