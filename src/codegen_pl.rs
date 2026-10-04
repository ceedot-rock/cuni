//! Perl backend — core-subset native seat over [`crate::codegen_core`].
//!
//! `$`-sigil vars, `my` bindings, `sub`s unpacking `@_`, `int()`-based
//! `cuni_div` (native `/` yields float), native floored `%`, `.` for string
//! concat, and `cuni_say` rendering bools as `True`/`False`.

use crate::ast::Program;
use crate::codegen_core::{generate as core_generate, LangSpec};

fn perl_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""), '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"), '\t' => r.push_str("\\t"), '\r' => r.push_str("\\r"),
            '$' => r.push_str("\\$"), '@' => r.push_str("\\@"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\x{{{:02x}}}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"'); r
}

const SPEC: LangSpec = LangSpec {
    comment: "#",
    header: "sub cuni_say { my ($x,$ty)=@_; print(($ty eq \"bool\" ? ($x ? \"True\" : \"False\") : $x), \"\\n\"); }\nsub cuni_div { my ($a,$b)=@_; return int($a/$b); }\n",
    main_open: "", main_close: "",
    let_fmt: "my {name} = {expr};", mut_fmt: "my {name} = {expr};", assign_fmt: "{name} = {expr};",
    fn_open_fmt: "sub {name} {\n    my ({params}) = @_;",
    fn_close: "}",
    ret_fmt: "return {expr};",
    if_open_fmt: "if ({cond}) {", else_open: "} else {", block_close: "}",
    while_open_fmt: "while ({cond}) {",
    say_fmt: "cuni_say({expr}, '{ty}');",
    call_fmt: "{name}({args})",
    true_lit: "1", false_lit: "0",
    str_quote: perl_str,
    int_div_helper: Some("cuni_div"), // Perl `/` yields float; int() truncates toward zero.
    int_mod_helper: None, // Perl `%` already floors like Python.
    str_eq_op: "eq",
    str_ne_op: "ne",
    str_lt_op: "lt",
    str_gt_op: "gt",
    str_le_op: "le",
    str_ge_op: "ge",
    concat_fmt: "({l} . {r})",
    var_prefix: "$", indent: "    ",
};

pub fn generate(program: &Program) -> Result<String, String> {
    core_generate(program, &SPEC)
}
