//! C# backend — core-subset native seat over [`crate::codegen_core`].
//!
//! Emits a complete console program (`class Program` + `static void Main`):
//! - `dynamic` locals, params, and returns: CuNi's core values are
//!   int/str/bool, and the DLR dispatches `+ - * / %`, the comparisons,
//!   and `&&`/`||`/`!` with exactly CuNi's semantics on those types.
//! - CuNi `str` is the `S` struct (a thin wrapper over `System.String`):
//!   C# defines `==`/`!=` for `string` but NOT `<`/`>`/`<=`/`>=`, so the
//!   wrapper overloads all six comparisons (ordinal, matching the py seat)
//!   plus `+` concatenation. Literals emit as `new S("...")`.
//! - int `/` truncates toward zero natively (CuNi); `%` goes through
//!   `cuni_mod` for Python-floored semantics (C#'s `%` truncates like C).
//! - `cuni_say(x, ty)` prints booleans as `True`/`False`; the `ty` tag comes
//!   from the core emitter, so no runtime type dispatch is needed.
//! - Integer literals are post-suffixed with `L`: CuNi `int` is 64-bit, while
//!   a bare C# literal is 32-bit and would silently wrap on overflow where
//!   the py seat keeps growing. String contents are never touched.
//! - `def` parameters get an explicit `dynamic` type via a small
//!   post-process — the core `LangSpec` substitutes parameter *names* only,
//!   and C# requires a type per parameter.
//!
//! Refuses everything beyond the core subset (see codegen_core docs). A
//! top-level `ret` is a C# compile error (loud, never silently dropped), as
//! is a bare `ret` with no value.

use crate::ast::Program;
use crate::codegen_core::{generate as core_generate, LangSpec};

/// A CuNi string literal: `new S("...")`. `S` (header) overloads the six
/// comparisons ordinally plus `+`, since C# gives `string` no `<`/`>`/`<=`/`>=`.
fn cs_str(s: &str) -> String {
    let mut r = String::from("new S(\"");
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
    r.push_str("\")");
    r
}

/// Suffix bare integer literals with `L` so every integer in the program is
/// 64-bit (CuNi `int` is i64). String literals, identifiers containing digit
/// runs (`x1`), and already-suffixed literals are left alone.
fn long_literals(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 32);
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut in_str = false;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            // Only a *leading* digit run is a literal: the `1` in `x1` is
            // skipped via the previous-character check.
            let prev = out.chars().last();
            let is_lead = !matches!(prev, Some(p) if p.is_ascii_alphanumeric() || p == '_');
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let already_suffixed =
                chars.get(j).map_or(false, |d| d.is_ascii_alphabetic());
            out.push_str(&chars[i..j].iter().collect::<String>());
            if is_lead && !already_suffixed {
                out.push('L');
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Give `def` parameters explicit `dynamic` types. The core emits
/// `static dynamic name(p1, p2) {`; C# needs `static dynamic name(dynamic p1,
/// dynamic p2) {`. Parameters that already carry a type (the `cuni_*`
/// helpers in the header) are left alone.
fn type_def_params(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    let marker = "static dynamic ";
    while let Some(pos) = rest.find(marker) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + marker.len()..];
        let parens = after
            .find('(')
            .and_then(|po| after[po + 1..].find(')').map(|pc| (po, po + 1 + pc)));
        match parens {
            Some((po, pc)) => {
                let name = &after[..po];
                let params = &after[po + 1..pc];
                let fixed = params
                    .split(',')
                    .map(|p| {
                        let p = p.trim();
                        if p.is_empty() || p.contains(' ') {
                            p.to_string()
                        } else {
                            format!("dynamic {}", p)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(marker);
                out.push_str(name);
                out.push('(');
                out.push_str(&fixed);
                out.push(')');
                rest = &after[pc + 1..];
            }
            None => {
                out.push_str(marker);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

const SPEC: LangSpec = LangSpec {
    comment: "//",
    // The class opens here; `def`s land inside it, then `Main` closes the
    // class via main_close. `S` is CuNi's `str`: C# has no `<`/`>`/`<=`/`>=`
    // on `string`, so the wrapper overloads all six comparisons (ordinal)
    // plus `+` concatenation; `ToString` keeps `say` printing the raw text.
    header: "using System;\n\nstruct S {\n    public string v;\n    public S(string v) { this.v = v; }\n    public override string ToString() { return v; }\n    public static S operator +(S a, S b) { return new S(a.v + b.v); }\n    public static bool operator ==(S a, S b) { return a.v == b.v; }\n    public static bool operator !=(S a, S b) { return !(a == b); }\n    public static bool operator <(S a, S b) { return string.CompareOrdinal(a.v, b.v) < 0; }\n    public static bool operator >(S a, S b) { return string.CompareOrdinal(a.v, b.v) > 0; }\n    public static bool operator <=(S a, S b) { return string.CompareOrdinal(a.v, b.v) <= 0; }\n    public static bool operator >=(S a, S b) { return string.CompareOrdinal(a.v, b.v) >= 0; }\n}\n\nclass Program {\n    static void cuni_say(dynamic x, string ty) {\n        if (ty == \"bool\") { Console.WriteLine((bool)x ? \"True\" : \"False\"); }\n        else { Console.WriteLine(x); }\n    }\n    static dynamic cuni_mod(dynamic a, dynamic b) { return ((a % b) + b) % b; }\n",
    main_open: "    static void Main() {",
    main_close: "    }\n}",
    let_fmt: "dynamic {name} = {expr};",
    mut_fmt: "dynamic {name} = {expr};",
    assign_fmt: "{name} = {expr};",
    fn_open_fmt: "    static dynamic {name}({params}) {",
    fn_close: "    }",
    ret_fmt: "return {expr};",
    if_open_fmt: "if ({cond}) {",
    else_open: "} else {",
    block_close: "}",
    while_open_fmt: "while ({cond}) {",
    say_fmt: "cuni_say({expr}, \"{ty}\");",
    call_fmt: "{name}({args})",
    true_lit: "true",
    false_lit: "false",
    str_quote: cs_str,
    // C# `/` on integers truncates toward zero: exactly CuNi.
    int_div_helper: None,
    // C# `%` truncates like C; CuNi wants Python-floored.
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
    let src = core_generate(program, &SPEC)?;
    Ok(long_literals(&type_def_params(&src)))
}
