//! Oddity-matrix hard-fail diagnostics (Universal AST v0 §3).
//!
//! Exactness refuse is sacred — there is **no approximate mode**. These helpers
//! label refuse messages with the matrix row so Rider/Studio surfaces can cite
//! the category instead of a generic lex/parse error.
//!
//! Measured gaps only: this module does not claim the full oddity corpus is done.

/// Format a labeled oddity hard-fail with a concrete fix-it.
pub fn refuse(category: &str, detail: &str, fix_it: &str) -> String {
    format!(
        "oddity hard-fail [{category}]: {detail} — fix-it: {fix_it} (docs/UNIVERSAL_AST_V0.md §3; no approximate mode)"
    )
}

pub const POINTERS: &str = "pointers";
pub const ASYNC: &str = "async";
pub const MACROS: &str = "macros";
pub const OWNERSHIP: &str = "ownership";
pub const PROTOTYPES: &str = "prototypes";

/// Portable-core idents that must hard-fail (not map) when they appear as names
/// or expression primaries. Absence from the grammar is not enough — alien
/// source can still spell them as `Ident` tokens.
pub fn async_oddity_ident(name: &str) -> bool {
    matches!(name, "async" | "await" | "spawn")
}

pub fn prototype_oddity_ident(name: &str) -> bool {
    matches!(name, "__proto__" | "prototype")
}

/// C / Rust preprocessor-shaped directive word after `#` (optional space).
/// Ordinary `# comment` lines stay comments; these hard-fail as macros.
pub fn macro_directive_word(word: &str) -> bool {
    matches!(
        word,
        "define"
            | "include"
            | "ifdef"
            | "ifndef"
            | "endif"
            | "pragma"
            | "undef"
            | "elif"
            | "else"
            | "error"
            | "warning"
            | "line"
            | "macro"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuse_message_names_category_and_fixit() {
        let m = refuse(POINTERS, "raw `&`", "use opaque indices");
        assert!(m.contains("oddity hard-fail [pointers]"));
        assert!(m.contains("fix-it:"));
        assert!(m.contains("no approximate mode"));
    }

    #[test]
    fn hash_comment_words_are_not_directives() {
        assert!(!macro_directive_word("This"));
        assert!(!macro_directive_word("exactness"));
        assert!(macro_directive_word("define"));
        assert!(macro_directive_word("include"));
    }
}
