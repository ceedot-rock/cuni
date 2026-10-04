//! Julia backend — core-subset native seat over [`crate::codegen_core`].
//!
//! Emits plain Julia (runs under `julia script.jl` with no packages):
//! `println`, `function`/`end`, `if`/`end`, `while`/`end`, and `*` for
//! string concatenation.
//!
//! Semantic choices:
//! - Julia `/` on ints yields a float, so CuNi's truncating-toward-zero int
//!   division goes through the builtin `div` (`int_div_helper`), which
//!   truncates toward zero (verified: `div(-7, 2) == -3`).
//! - Julia `%` is `rem` (sign of the dividend), but the builtin `mod` is
//!   Python-floored (verified: `mod(-7, 2) == 1`, `mod(7, -2) == -1`), so
//!   `int_mod_helper` is just `mod`.
//! - `cuni_say(x, ty)` prints via `println`; the `ty` tag renders bools as
//!   `True`/`False` since Julia's native literals are `true`/`false`.
//! - The script body is wrapped in a bare `let ... end` block: at Julia's
//!   global scope a `while` loop cannot assign to an outer variable (hard
//!   scope), but inside `let` the loop sees the enclosing locals. `def`s
//!   stay top-level `function`s.
//! - `$` is escaped in string literals (Julia interpolates `$`).
//!
//! Honest caveat: Julia ints are `Int64`, so values are exact only within
//! ±2^63 (wraparound beyond); that is Julia's own arithmetic, not a CuNi lie.
//!
//! Refuses everything beyond the core subset (see codegen_core docs).

use crate::ast::Program;
use crate::codegen_core::{generate as core_generate, LangSpec};

fn jl_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            '$' => r.push_str("\\$"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\u{:04x}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

const SPEC: LangSpec = LangSpec {
    comment: "#",
    header: "function cuni_say(x, ty)\n    if ty == \"bool\"\n        println(x ? \"True\" : \"False\")\n    else\n        println(x)\n    end\nend\n",
    main_open: "let",
    main_close: "end",
    let_fmt: "{name} = {expr}",
    mut_fmt: "{name} = {expr}",
    assign_fmt: "{name} = {expr}",
    fn_open_fmt: "function {name}({params})",
    fn_close: "end",
    ret_fmt: "return {expr}",
    if_open_fmt: "if {cond}",
    else_open: "else",
    block_close: "end",
    while_open_fmt: "while {cond}",
    say_fmt: "cuni_say({expr}, \"{ty}\")",
    call_fmt: "{name}({args})",
    true_lit: "true",
    false_lit: "false",
    str_quote: jl_str,
    // Julia `/` on ints yields a float; div() truncates toward zero (CuNi).
    int_div_helper: Some("div"),
    // Julia `%` is rem (sign of dividend); mod() is Python-floored (CuNi).
    int_mod_helper: Some("mod"),
    str_eq_op: "==",
    str_ne_op: "!=",
    str_lt_op: "<",
    str_gt_op: ">",
    str_le_op: "<=",
    str_ge_op: ">=",
    concat_fmt: "({l} * {r})",
    var_prefix: "",
    indent: "    ",
};

pub fn generate(program: &Program) -> Result<String, String> {
    core_generate(program, &SPEC)
}
