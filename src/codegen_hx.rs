//! Haxe backend — core-subset native seat over [`crate::codegen_core`].
//!
//! Haxe has no top-level statements, so the header opens `class Main {`
//! (the emitted file MUST be saved as `Main.hx`), `def`s become
//! `static function`s, and the script body becomes `static function main()`.
//! Run with `haxe --interp -cp <dir> --main Main`, or compile to JS
//! (`haxe -cp <dir> --main Main --js out.js`) and run with node.
//!
//! Semantic choices:
//! - Haxe `/` on ints yields a Float, so CuNi's truncating-toward-zero int
//!   division goes through `cuni_div(a, b) = Std.int(a / b)` (`Std.int`
//!   truncates toward zero).
//! - Haxe `%` follows the dividend (C-like), so `cuni_mod(a, b) =
//!   ((a % b) + b) % b` makes it Python-floored (CuNi).
//! - `Sys.println` is used, NOT `trace` (trace adds file/line decoration).
//!   The `ty` tag renders bools as `True`/`False`.
//! - Double-quoted strings are literal in Haxe (interpolation is
//!   single-quote only), so `$` needs no escaping.
//!
//! Honest caveat: Haxe `Int` is 32-bit on the JS/interp targets
//! (wraparound beyond ±2^31); that is Haxe's own arithmetic, not a CuNi lie.
//!
//! Refuses everything beyond the core subset (see codegen_core docs).

use crate::ast::Program;
use crate::codegen_core::{generate as core_generate, LangSpec};

fn hx_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\u{:04x}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

const SPEC: LangSpec = LangSpec {
    comment: "//",
    // Opens `class Main {`; defs land inside as static functions, then
    // main_open adds `static function main() {` and main_close shuts both.
    header: "class Main {\nstatic function cuni_say(x:Dynamic, ty:String):Void {\n    if (ty == \"bool\") Sys.println(x ? \"True\" : \"False\"); else Sys.println(Std.string(x));\n}\nstatic function cuni_div(a:Int, b:Int):Int { return Std.int(a / b); }\nstatic function cuni_mod(a:Int, b:Int):Int { return ((a % b) + b) % b; }\n",
    main_open: "static function main() {",
    main_close: "}\n}",
    let_fmt: "var {name} = {expr};",
    mut_fmt: "var {name} = {expr};",
    assign_fmt: "{name} = {expr};",
    fn_open_fmt: "static function {name}({params}) {",
    fn_close: "}",
    ret_fmt: "return {expr};",
    if_open_fmt: "if ({cond}) {",
    else_open: "} else {",
    block_close: "}",
    while_open_fmt: "while ({cond}) {",
    say_fmt: "cuni_say({expr}, \"{ty}\");",
    call_fmt: "{name}({args})",
    true_lit: "true",
    false_lit: "false",
    str_quote: hx_str,
    // Haxe `/` on ints yields Float; Std.int() truncates toward zero (CuNi).
    int_div_helper: Some("cuni_div"),
    // Haxe `%` follows the dividend; cuni_mod makes it Python-floored.
    int_mod_helper: Some("cuni_mod"),
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
