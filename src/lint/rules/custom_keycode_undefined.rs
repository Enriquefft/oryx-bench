//! `custom-keycode-undefined` — visual layout binds USERnn but no overlay defines it.
//!
//! A `USERnn` slot is "defined" if any of the following is true:
//!
//! 1. `features.toml` has a `[[macros]]` entry whose `slot = "USERnn"`
//!    (Tier 1 declarative form).
//! 2. A Tier 2 `.zig` file in `overlay/` mentions `USERnn` — the
//!    overlay has a dispatch arm for that slot.
//! 3. A Tier 2′ vendored `.c` file in `overlay/` mentions `USERnn`.
//!
//! The previous version of the rule only checked (1), producing a
//! false-positive error whenever the user legitimately dispatched a
//! custom keycode from Zig — exactly the case the rule's own
//! `fix_example` documents as valid.

use std::collections::HashSet;

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "custom-keycode-undefined"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn description(&self) -> &'static str {
        "The visual layout binds a `USERnn` keycode but no `[[macros]]` entry, `.zig`, or vendored `.c` file in `overlay/` defines what `USERnn` does."
    }
    fn why_bad(&self) -> &'static str {
        "Pressing the key does nothing."
    }
    fn fix_example(&self) -> &'static str {
        "Either add a `[[macros]]` entry in `features.toml` with `slot = \"USERnn\"`, or add a Tier 2 dispatch arm in an `overlay/*.zig` file, or remove the binding from the visual layout."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        // Union of "slots defined in features.toml macros" and "slot
        // names mentioned in any overlay Tier 2 file". The latter is a
        // textual scan — we don't parse Zig or C, we just check for
        // the literal token. False-negative risk is zero (we'd have
        // to invent a USERnn mention) and the only false-negative for
        // the rule would be a user who renames a slot without updating
        // Zig, which is a separate bug class.
        let mut defined: HashSet<String> = ctx
            .features
            .macros
            .iter()
            .filter_map(|m| m.slot.clone())
            .collect();
        defined.extend(scan_overlay_for_user_slots(ctx.project.overlay_dir()));

        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            for (idx, key) in layer.keys.iter().enumerate() {
                for slot in [&key.tap, &key.hold, &key.double_tap, &key.tap_hold] {
                    if let Some(CanonicalAction::Custom(n)) = slot {
                        let slot_name = format!("USER{:02}", n);
                        if !defined.contains(&slot_name) {
                            out.push(Issue {
                                rule_id: self.id().to_string(),
                                severity: self.severity(),
                                message: format!(
                                    "{slot_name} is bound in visual layout but no overlay defines it"
                                ),
                                layer: Some(layer.name.clone()),
                                position_index: Some(idx),
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

/// Return every `USERnn` slot name (e.g. `USER00`, `USER12`) that
/// appears as a literal token anywhere under `overlay/`'s `.zig` or
/// `.c` files. Used by the rule above to treat "mentioned in Tier 2"
/// as "defined".
///
/// Files that fail to open are silently skipped — unlike the
/// `process-record-user-collision` rule, here a read failure doesn't
/// affect correctness because the worst case is a false-positive
/// "undefined" report (which the user can investigate).
fn scan_overlay_for_user_slots(overlay_dir: std::path::PathBuf) -> HashSet<String> {
    let mut out = HashSet::new();
    if !overlay_dir.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(&overlay_dir).into_iter().flatten() {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if !matches!(ext, "zig" | "c" | "h") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        extract_user_tokens(&contents, &mut out);
    }
    out
}

/// Parse out every `USER<nn>` token from a source file. A token is
/// `USER` followed by at least one ASCII digit AND bounded on both
/// sides by a non-identifier byte (or end-of-file). This catches
/// `USER00` in a case arm, `USER12` in a comment, etc., while
/// rejecting:
///
/// - `MY_USER00` — preceding `_` is an ident byte (left-boundary fail).
/// - `USER05suffix` — trailing `s` is an ident byte (right-boundary fail).
/// - `USER1USER2` — neither half has clean boundaries; both rejected.
fn extract_user_tokens(source: &str, out: &mut HashSet<String>) {
    let bytes = source.as_bytes();
    let mut i = 0;
    // `<=` so a file ending in exactly `USERnn` (5+ bytes, no trailing
    // whitespace) is still scanned. Using `<` would skip the final
    // 4-byte window unnecessarily.
    while i + 4 <= bytes.len() {
        // Fast path: find the next `U` and check.
        if &bytes[i..i + 4] != b"USER" {
            i += 1;
            continue;
        }
        // Left boundary: preceding byte must not be an identifier
        // character, otherwise this is a suffix of some longer
        // identifier like `MY_USER00`.
        let left_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
        if !left_ok {
            i += 4;
            continue;
        }
        let mut j = i + 4;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == i + 4 {
            // Zero digits after `USER` — not a slot reference at all
            // (e.g. the bare token `USER`, or `USER_FOO`). Skip past
            // this `USER` so we don't re-scan it on the next iteration.
            i += 4;
            continue;
        }
        // Right boundary: the byte AFTER the digits must not be an
        // identifier character. Without this check `USER1USER2`
        // would extract `USER01` (because we'd see `USER` + `1` and
        // stop), even though the whole thing is a single C/Zig
        // identifier and the user clearly didn't write a slot
        // reference. Same for `USER05suffix`.
        let right_ok = j == bytes.len() || !is_ident_byte(bytes[j]);
        if !right_ok {
            // Skip past the whole word (digits + ident continuation)
            // so the inner cursor doesn't re-scan the rejected match.
            i = j;
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            continue;
        }
        // At least one digit AND clean boundaries — it's a slot
        // reference. Normalize to two-digit form to match what
        // codegen and the macro-slot assignment produce.
        let num: u32 = std::str::from_utf8(&bytes[i + 4..j])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        out.insert(format!("USER{num:02}"));
        i = j;
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_basic_user_token() {
        let mut out = HashSet::new();
        extract_user_tokens("case USER00: return false;", &mut out);
        assert!(out.contains("USER00"));
    }

    #[test]
    fn normalizes_single_digit_to_two_digit() {
        let mut out = HashSet::new();
        extract_user_tokens("USER5", &mut out);
        assert!(out.contains("USER05"));
    }

    #[test]
    fn ignores_non_boundary_match() {
        // MY_USER00 should NOT match USER00 — the preceding `_` makes
        // it part of a larger identifier.
        let mut out = HashSet::new();
        extract_user_tokens("MY_USER00 = 1;", &mut out);
        assert!(out.is_empty(), "spurious match: {out:?}");
    }

    #[test]
    fn extracts_multiple_tokens() {
        let mut out = HashSet::new();
        extract_user_tokens(
            "switch (kc) { case USER00: ...; case USER12: ...; }",
            &mut out,
        );
        assert!(out.contains("USER00"));
        assert!(out.contains("USER12"));
    }

    #[test]
    fn ignores_user_without_digits() {
        let mut out = HashSet::new();
        extract_user_tokens("pub fn USER() void {}", &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn rejects_match_followed_by_ident_byte() {
        // Regression: `USER05suffix` is one identifier in C/Zig
        // (the trailing `s` is an identifier byte), so the scanner
        // must NOT extract `USER05`. Same logic as the existing
        // left-boundary check on `MY_USER00`.
        let mut out = HashSet::new();
        extract_user_tokens("if (kc == USER05suffix) return;", &mut out);
        assert!(
            out.is_empty(),
            "USER05suffix should not extract USER05; got {out:?}"
        );
    }

    #[test]
    fn rejects_concatenated_user_tokens() {
        // Regression: `USER1USER2` is one identifier — the `U` of
        // the second USER is an identifier byte, so the first half's
        // right boundary fails. Both halves should be rejected.
        let mut out = HashSet::new();
        extract_user_tokens("USER1USER2", &mut out);
        assert!(
            out.is_empty(),
            "USER1USER2 should extract nothing; got {out:?}"
        );
    }

    #[test]
    fn match_at_end_of_file_with_no_trailing_whitespace() {
        // Regression for the loop bound: a file ending in exactly
        // `USER05` (no trailing newline / whitespace) must still
        // produce the match. The previous `<` bound silently
        // skipped the final 4-byte window on small files.
        let mut out = HashSet::new();
        extract_user_tokens("dispatch USER05", &mut out);
        assert!(out.contains("USER05"));
    }

    #[test]
    fn match_at_exact_end_of_buffer() {
        // The shortest valid input ending in a slot reference.
        let mut out = HashSet::new();
        extract_user_tokens("USER0", &mut out);
        assert!(out.contains("USER00"));
    }

    #[test]
    fn match_followed_by_punctuation_extracts_correctly() {
        // Real-world C: `case USER05:` and `(USER12)` and `USER12;`
        // — all delimited by non-identifier bytes, all should match.
        let mut out = HashSet::new();
        extract_user_tokens(
            "case USER05: return; if (kc == USER12) {} else if (kc == USER31) {}",
            &mut out,
        );
        assert!(out.contains("USER05"));
        assert!(out.contains("USER12"));
        assert!(out.contains("USER31"));
        assert_eq!(out.len(), 3);
    }
}
