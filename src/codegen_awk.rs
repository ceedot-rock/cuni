//! Awk backend — core-subset native seat over [`crate::codegen_core`].
//!
//! Emits a gawk program: a `BEGIN` block drives everything, `cuni_say`
//! dispatches on the type tag (booleans print `True`/`False`), `cuni_div`
//! truncates toward zero via `int()`, `cuni_mod` floors, and string
//! concatenation is juxtaposition. Run with `gawk -f`.
//! Refuses everything beyond the core subset (see codegen_core docs).
//!
//! Precision note: awk numbers are doubles, exact for integers below 2^53 —
//! far above the core fixtures' range. Magnitudes beyond that would lose
//! precision, so they are out of scope for this seat.

use crate::ast::Program;
use crate::codegen_core::{generate as core_generate, LangSpec};

fn awk_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\{:03o}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

const SPEC: LangSpec = LangSpec {
    comment: "#",
    header: "function cuni_div(a, b) { return int(a / b); }\nfunction cuni_mod(a, b) { return (a % b + b) % b; }\nfunction cuni_say(x, ty) {\n    if (ty == \"bool\") { print (x ? \"True\" : \"False\"); }\n    else { print x; }\n}\n",
    main_open: "BEGIN {",
    main_close: "}",
    let_fmt: "{name} = {expr};",
    mut_fmt: "{name} = {expr};",
    assign_fmt: "{name} = {expr};",
    fn_open_fmt: "function {name}({params}) {",
    fn_close: "}",
    ret_fmt: "return {expr};",
    if_open_fmt: "if ({cond}) {",
    else_open: "} else {",
    block_close: "}",
    while_open_fmt: "while ({cond}) {",
    say_fmt: "cuni_say({expr}, \"{ty}\");",
    call_fmt: "{name}({args})",
    true_lit: "1",
    false_lit: "0",
    str_quote: awk_str,
    // awk `/` is floating-point; int() truncates toward zero (CuNi).
    int_div_helper: Some("cuni_div"),
    // awk `%` truncates like C; CuNi wants Python-floored.
    int_mod_helper: Some("cuni_mod"),
    // awk compares string constants lexicographically with these operators.
    str_eq_op: "==",
    str_ne_op: "!=",
    str_lt_op: "<",
    str_gt_op: ">",
    str_le_op: "<=",
    str_ge_op: ">=",
    // awk concatenates strings by juxtaposition.
    concat_fmt: "({l} {r})",
    var_prefix: "",
    indent: "    ",
};

pub fn generate(program: &Program) -> Result<String, String> {
    core_generate(program, &SPEC)
}
