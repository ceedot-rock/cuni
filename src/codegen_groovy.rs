//! Groovy backend — core-subset native seat over [`crate::codegen_core`].
//!
//! Emits a Groovy script: top-level statements run directly, `def` helpers
//! (`cuni_say` with bool → `True`/`False`, `cuni_div`, `cuni_mod`), untyped
//! `def name(params)` functions, `println`, `while`. Refuses everything
//! beyond the core subset (see codegen_core docs).
//!
//! Semantics notes (verified against the py seat, which is the gate's gold):
//! - Groovy `/` on ints is BigDecimal division, so integer `/` goes through
//!   `cuni_div` (`a.intdiv(b)`, truncated toward zero — CuNi semantics).
//! - Groovy `%` on ints is the truncated remainder, so every `%` goes through
//!   `cuni_mod` (Python-floored `((a % b) + b) % b`; a zero divisor throws,
//!   crashing the seat the way Python's `ZeroDivisionError` crashes the
//!   gold — a run failure, never silent divergence).
//! - `==` on Strings is `.equals` in Groovy (correct); `<` on Strings is
//!   `compareTo` (same order as the py seat for the ASCII fixtures).
//! - String literals escape `$`: Groovy `"..."` interpolates `$x`/`${...}`.
//!
//! Wiring note: `emit.rs`/`main.rs` dispatch is left to the parent — this file
//! only provides `generate`.

use crate::ast::Program;
use crate::codegen_core::{generate as core_generate, LangSpec};

fn groovy_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            // Groovy "..." interpolates $x and ${...}; escape it.
            '$' => r.push_str("\\$"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\u{:04x}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

const SPEC: LangSpec = LangSpec {
    comment: "//",
    header: "def cuni_div(a, b) { a.intdiv(b) }\ndef cuni_mod(a, b) { ((a % b) + b) % b }\ndef cuni_say(x, ty) { if (ty == \"bool\") { println(x ? \"True\" : \"False\") } else { println(x) } }\n",
    // Groovy scripts run top-level statements directly: no main wrapper.
    main_open: "",
    main_close: "",
    let_fmt: "def {name} = {expr}",
    mut_fmt: "def {name} = {expr}",
    assign_fmt: "{name} = {expr}",
    fn_open_fmt: "def {name}({params}) {",
    fn_close: "}",
    ret_fmt: "return {expr}",
    if_open_fmt: "if ({cond}) {",
    else_open: "} else {",
    block_close: "}",
    while_open_fmt: "while ({cond}) {",
    say_fmt: "cuni_say({expr}, '{ty}')",
    call_fmt: "{name}({args})",
    true_lit: "true",
    false_lit: "false",
    str_quote: groovy_str,
    // Groovy `/` on ints is BigDecimal division; intdiv() truncates (CuNi).
    int_div_helper: Some("cuni_div"),
    // Groovy `%` on ints truncates like Java; CuNi wants Python-floored.
    int_mod_helper: Some("cuni_mod"),
    // Groovy `==` is .equals (right for strings); `<` is compareTo.
    str_eq_op: "==",
    str_ne_op: "!=",
    str_lt_op: "<",
    str_gt_op: ">",
    str_le_op: "<=",
    str_ge_op: ">=",
    concat_fmt: "({l} + {r})",
    var_prefix: "",
    indent: "    ",
};

pub fn generate(program: &Program) -> Result<String, String> {
    core_generate(program, &SPEC)
}
