//! Identifier sanitization for layer names → C identifiers.
//!
//! Used by both the codegen layer (when emitting `enum layers { ... }`)
//! and the `layer-name-collision` lint rule. Lives in `schema/` so the
//! codegen doesn't have to import from `lint::rules`.

/// Sanitize an Oryx layer title to a valid C identifier.
///
/// Rules (ARCHITECTURE.md#layer-identity-sanitization):
///
/// 1. Uppercase
/// 2. Non-alphanumeric → underscore
/// 3. Collapse repeated underscores
/// 4. Strip leading/trailing underscores
/// 5. Prefix with `L_` if the result starts with a digit
pub fn sanitize_c_ident(title: &str) -> String {
    let upper = title.to_uppercase();
    let mut out = String::new();
    let mut last_was_underscore = false;
    for ch in upper.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_underscore = false;
        } else if !last_was_underscore {
            out.push('_');
            last_was_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    let result = if trimmed
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        format!("L_{trimmed}")
    } else {
        trimmed
    };
    if result.is_empty() {
        "LAYER".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_spaces_and_symbols() {
        assert_eq!(sanitize_c_ident("Sym + Num!"), "SYM_NUM");
        assert_eq!(sanitize_c_ident("Sym Num"), "SYM_NUM");
        assert_eq!(sanitize_c_ident("Gaming"), "GAMING");
    }

    #[test]
    fn prefixes_leading_digit() {
        assert_eq!(sanitize_c_ident("1 Fun"), "L_1_FUN");
    }

    #[test]
    fn empty_string_fallback() {
        assert_eq!(sanitize_c_ident(""), "LAYER");
    }
}
