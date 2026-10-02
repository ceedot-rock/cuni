//! F# backend — core-subset native seat over [`crate::codegen_core`].
//!
//! Emits an `.fsx` script, run with `dotnet fsi` (no compilation step):
//! - Real `mutable` bindings and `while` loops; `let rec` defs (recursion
//!   needs `rec` — exercised by `fact` in the fixtures).
//! - `ret` becomes `raise (CuniRet (box e))` inside a per-def
//!   `try ... with CuniRet v -> unbox v`: the honest early-return mapping
//!   for a language with no `return` statement. The exit is non-local, so
//!   `ret` inside `if`/`while` bodies behaves exactly.
//! - Defs are `let rec` with explicit type annotations (`int64`/`string`/
//!   `bool`, from the CuNi signatures): without annotations F# would default
//!   an unconstrained numeric parameter to 32-bit `int`, and `inline` (the
//!   other fix) is unavailable — F# rejects `inline` on recursive defs
//!   (FS1114), and `fact` recurses. The annotations are added by a small
//!   post-process over the AST; the core has already refused any def whose
//!   types aren't int/str/bool.
//! - `printfn` printing; booleans render as `True`/`False` via the core's
//!   `ty` tag (`%O` handles int64 and string values).
//! - int `/` truncates toward zero natively (CuNi); `%` goes through an
//!   inline `cuni_mod` for Python-floored semantics (F#'s `%` truncates).
//!
//! Post-passes over the emitted source:
//! - `type_defs`: rewrite each `def` opening line with explicit F# type
//!   annotations (see below);
//! - `fs_post`: integer literals gain an `L` suffix (64-bit, see above;
//!   string contents untouched), the core's `(!x)` spelling of `not` becomes
//!   `(not x)` (in F#, `!` dereferences a `ref` cell and does not negate
//!   booleans), and the core's C-style `==` / `!=` (emitted for non-string
//!   operands) become F#'s `=` / `<>`;
//! - `fs_indent_try`: re-indent `try` bodies one level to satisfy F#'s
//!   offside rule — the core emits `def` bodies only one level under the
//!   `fn_open_fmt` line that already contains `try`.
//!
//! Refuses everything beyond the core subset (see codegen_core docs). A bare
//! `ret` with no value has no mapping and fails at run time — loud, never
//! silently dropped.

use crate::ast::{Item, Program, Type};
use crate::codegen_core::{generate as core_generate, LangSpec};

fn fs_str(s: &str) -> String {
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

/// Post-pass over the emitted source, outside string literals:
/// - `42` -> `42L` (64-bit integer literals; see module docs);
/// - `(!x)` -> `(not x)` (F# `!` is ref-deref, not boolean negation);
/// - `==` -> `=`, `!=` -> `<>` (the core emits C-style equality for
///   non-string operands; F# only has `=` / `<>`).
fn fs_post(src: &str) -> String {
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
        // `not`: the core spells unary `not` as `(!x)`. `!=` is checked
        // first so a (never-emitted) `(!=` can't misfire; the core always
        // emits `(l != r)` with the `(` before the operand, so a real `!=`
        // never starts with `(`.
        if c == '!' && chars.get(i + 1) == Some(&'=') {
            out.push_str("<>");
            i += 2;
            continue;
        }
        if c == '(' && chars.get(i + 1) == Some(&'!') {
            out.push_str("(not ");
            i += 2;
            continue;
        }
        if c == '=' && chars.get(i + 1) == Some(&'=') {
            out.push('=');
            i += 2;
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

const SPEC: LangSpec = LangSpec {
    comment: "//",
    header: "exception CuniRet of obj\nlet cuni_say (x: obj) (ty: string) =\n    if ty = \"bool\" then printfn \"%s\" (if unbox<bool> x then \"True\" else \"False\")\n    else printfn \"%O\" x\nlet inline cuni_mod (a, b) = ((a % b) + b) % b\n",
    // Top-level statements run directly in the .fsx script.
    main_open: "",
    main_close: "",
    let_fmt: "let {name} = {expr}",
    mut_fmt: "let mutable {name} = {expr}",
    assign_fmt: "{name} <- {expr}",
    // Tuple-style params: `let rec add (a, b) =`; call sites emit
    // `add(x, y)`, which applies the function to a tuple — valid F#.
    // `type_defs` (below) rewrites the opening line with real annotations.
    fn_open_fmt: "let rec {name} ({params}) =\n    try",
    fn_close: "    with CuniRet v -> unbox v",
    ret_fmt: "raise (CuniRet (box ({expr})))",
    if_open_fmt: "if {cond} then",
    else_open: "else",
    block_close: "",
    while_open_fmt: "while {cond} do",
    say_fmt: "cuni_say (box ({expr})) \"{ty}\"",
    call_fmt: "{name}({args})",
    true_lit: "true",
    false_lit: "false",
    str_quote: fs_str,
    // F# `/` on integers truncates toward zero: exactly CuNi.
    int_div_helper: None,
    // F# `%` truncates; CuNi wants Python-floored.
    int_mod_helper: Some("cuni_mod"),
    str_eq_op: "=",
    str_ne_op: "<>",
    str_lt_op: "<",
    str_gt_op: ">",
    str_le_op: "<=",
    str_ge_op: ">=",
    concat_fmt: "({l} + {r})",
    var_prefix: "",
    indent: "    ",
};

pub fn generate(program: &Program) -> Result<String, String> {
    // core_generate runs first: anything beyond int/str/bool is refused
    // there, so every def signature below is annotatable.
    let src = core_generate(program, &SPEC)?;
    let src = type_defs(program, &src)?;
    Ok(fs_indent_try(&fs_post(&src)))
}

/// Rewrite each `def` opening line with explicit F# type annotations:
/// `let rec add (a, b) =` -> `let rec add (a: int64, b: int64) : int64 =`.
/// Without annotations F# defaults unconstrained numerics to 32-bit `int`,
/// which would neither match the `L`-suffixed literals nor CuNi's i64.
fn type_defs(program: &Program, src: &str) -> Result<String, String> {
    fn fs_ty(ty: &Type) -> Result<&'static str, String> {
        match ty {
            Type::Named(n) if n == "int" => Ok("int64"),
            Type::Named(n) if n == "str" => Ok("string"),
            Type::Named(n) if n == "bool" => Ok("bool"),
            _ => Err("def type beyond int/str/bool: refusing".into()),
        }
    }
    let mut out = src.to_string();
    for item in &program.items {
        let Item::Def(f) = item else { continue };
        let rt = fs_ty(&f.ret_type)?;
        let names: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
        let mut typed = Vec::with_capacity(names.len());
        for p in &f.params {
            typed.push(format!("{}: {}", p.name, fs_ty(&p.ty)?));
        }
        let from = format!("let rec {} ({}) =", f.name, names.join(", "));
        let to = format!("let rec {} ({}) : {} =", f.name, typed.join(", "), rt);
        if !out.contains(&from) {
            return Err(format!("def {}: opening line not found; refusing", f.name));
        }
        out = out.replacen(&from, &to, 1);
    }
    Ok(out)
}

/// Re-indent `try` bodies one extra level. The core emits `def` bodies one
/// indent level under `fn_open_fmt`, but F#'s offside rule needs the `try`
/// block indented *under* the baked-in `try` line — so lines between
/// `    try` and `    with` gain 4 spaces. The shape is fixed by this
/// emitter's own `fn_open_fmt`/`fn_close`, never by user code.
fn fs_indent_try(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 32);
    let mut in_try = false;
    for line in src.split_inclusive('\n') {
        let stripped = line.trim_end_matches('\n');
        if stripped == "    try" {
            in_try = true;
            out.push_str(line);
        } else if in_try && stripped.starts_with("    with ") {
            in_try = false;
            out.push_str(line);
        } else if in_try && !stripped.is_empty() {
            out.push_str("    ");
            out.push_str(line);
        } else {
            out.push_str(line);
        }
    }
    out
}
