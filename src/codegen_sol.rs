//! Solidity backend — the blockchain contract writer.
//!
//! Emits a real, deployable Solidity smart contract (pragma ^0.8.0) from CuNi
//! source. This is genuine Solidity, not a Python lowering: `def` becomes a
//! `public pure` function, top-level statements become a public `run()`
//! function, and `say` becomes `emit Log*` events.
//!
//! Exactness notes:
//! - CuNi `int` is i64 with truncated `/` and `%` (Rust semantics).
//!   Solidity `int256` `/` and `%` are also truncated toward zero, so integer
//!   arithmetic matches exactly with no helpers.
//! - `float`, `list`, `map`, `opt`, and `??` (unwrap) are honestly refused:
//!   Solidity has no floats, and try/catch only works on external calls.
//! - `fail` becomes `revert`, matching CuNi's failure semantics.

use crate::ast::{
    BinOp, CallArg, EnumDecl, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr,
    TypDecl, Type, UnOp,
};
use std::collections::{HashMap, HashSet};

/// Solidity type of a CuNi variable, for `say` -> event routing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SolKind {
    Int,
    /// CuNi `dec`: scaled int256 (docs/DECIMAL.md).
    Dec,
    /// CuNi `time`: uint256 unix epoch seconds, UTC (docs/TIME.md).
    /// Non-negative only — negative times refuse at emit (docs/TIME.md §7).
    Time,
    Str,
    Bool,
    Other,
}

pub struct Codegen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, SolKind>,
    typ_names: HashSet<String>,
    enum_names: HashSet<String>,
    /// Does any emitted function body use `say` (needs events, not pure)?
    uses_emit: bool,
    /// Is `_cuni_itoa` needed (int interpolation)?
    needs_itoa: bool,
    /// Is `_cuni_dec_str` needed (dec say / interpolation)?
    needs_dec_str: bool,
    /// Is `_cuni_time_str` needed (time say / interpolation)?
    needs_time_str: bool,
    /// Are the time arithmetic/parse helpers needed?
    needs_time_helpers: bool,
    out: String,
}

impl Codegen {
    fn new(program: &Program) -> Self {
        let mut fn_names = HashSet::new();
        let mut fn_ret = HashMap::new();
        let mut typ_names = HashSet::new();
        let mut enum_names = HashSet::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    fn_names.insert(f.name.clone());
                    fn_ret.insert(f.name.clone(), kind_of_type(&f.ret_type));
                }
                Item::Typ(t) => {
                    typ_names.insert(t.name.clone());
                }
                Item::Enum(e) => {
                    enum_names.insert(e.name.clone());
                }
                _ => {}
            }
        }
        Codegen {
            fn_names,
            fn_ret,
            typ_names,
            enum_names,
            uses_emit: false,
            needs_itoa: false,
            needs_dec_str: false,
            needs_time_str: false,
            needs_time_helpers: false,
            out: String::new(),
        }
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn esc(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                c => r.push(c),
            }
        }
        r
    }
}

/// Map a CuNi type to its Solidity declaration type.
fn sol_type(ty: &Type) -> Result<String, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok("int256".into()),
            // `dec` is a scaled int256 — the natural fit (docs/DECIMAL.md §7).
            "dec" => Ok("int256".into()),
            // `time` is a uint256 epoch — non-negative only (docs/TIME.md §7).
            "time" => Ok("uint256".into()),
            "str" => Ok("string".into()),
            "bool" => Ok("bool".into()),
            "float" => Err("Solidity has no float type; refusing float".into()),
            other => Err(format!(
                "type `{}` has no Solidity mapping; refusing",
                other
            )),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{}` has no Solidity mapping; refusing",
            name
        )),
    }
}

fn kind_of_type(ty: &Type) -> SolKind {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => SolKind::Int,
            "dec" => SolKind::Dec,
            "time" => SolKind::Time,
            "str" => SolKind::Str,
            "bool" => SolKind::Bool,
            _ => SolKind::Other,
        },
        _ => SolKind::Other,
    }
}

fn kind_of_literal(e: &Expr) -> Option<SolKind> {
    match &e.kind {
        ExprKind::Int(_) => Some(SolKind::Int),
        ExprKind::Dec(_) => Some(SolKind::Dec),
        ExprKind::Time(_) => Some(SolKind::Time),
        ExprKind::Str(_) | ExprKind::InterpStr(_) => Some(SolKind::Str),
        ExprKind::Bool(_) => Some(SolKind::Bool),
        _ => None,
    }
}

pub fn generate(program: &Program) -> Result<String, String> {
    generate_named(program, "CuniContract")
}

pub fn generate_named(program: &Program, contract: &str) -> Result<String, String> {
    // The sol seat is uint256: negative time literals refuse at emit
    // (docs/TIME.md §7) — before emitting anything.
    crate::ast::check_time_literals_in_range(program, "sol")?;
    let mut g = Codegen::new(program);
    g.gen_program(program, contract)?;
    Ok(g.out)
}

impl Codegen {
    fn gen_program(&mut self, program: &Program, contract: &str) -> Result<(), String> {
        self.line(0, "// SPDX-License-Identifier: MIT");
        self.line(0, "pragma solidity ^0.8.0;");
        self.out.push('\n');
        self.line(0, "/// @title");
        self.line(
            0,
            "/// @notice Generated by the CuNi Solidity backend. Same source, same",
        );
        self.line(
            0,
            "/// behavior on every CuNi seat — or CuNi refuses to emit it.",
        );
        // Structs first (types used by functions).
        for item in &program.items {
            if let Item::Typ(t) = item {
                self.gen_struct(t)?;
            }
        }
        // Enums.
        for item in &program.items {
            if let Item::Enum(e) = item {
                self.gen_enum(e);
            }
        }
        self.line(0, &format!("contract {} {{", contract));
        self.line(1, "event LogInt(int256 value);");
        self.line(1, "event LogDec(string value);");
        self.line(1, "event LogTime(string value);");
        self.line(1, "event LogString(string value);");
        self.line(1, "event LogBool(bool value);");
        self.out.push('\n');
        // Functions.
        for item in &program.items {
            if let Item::Def(f) = item {
                self.gen_def(f)?;
                self.out.push('\n');
            }
        }
        // Top-level statements -> run().
        let top: Vec<&Stmt> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Stmt(s) => Some(s),
                _ => None,
            })
            .collect();
        if !top.is_empty() {
            self.line(
                1,
                "/// @notice Executes the contract's top-level program. Emits events.",
            );
            self.line(1, "function run() public {");
            let mut scope = HashMap::new();
            for s in top {
                self.gen_stmt(2, s, &mut scope)?;
            }
            self.line(1, "}");
        }
        // String helpers, emitted last so top-level `say` can set the
        // flags in time (docs/DECIMAL.md §6).
        self.emit_helpers();
        self.line(0, "}");
        Ok(())
    }

    /// Emit `_cuni_itoa` / `_cuni_dec_str` when interpolation or `say`
    /// needed them. Called after all statements are generated.
    fn emit_helpers(&mut self) {
            // _cuni_itoa helper if any interpolation needs it.
            if self.needs_dec_str {
                // _cuni_dec_str reuses _cuni_itoa for the integer part.
                self.needs_itoa = true;
            }
            if self.needs_itoa {
                self.line(
                    1,
                    "/// @notice int256 -> decimal string (for interpolated output).",
                );
                self.line(
                    1,
                    "function _cuni_itoa(int256 v) internal pure returns (string memory) {",
                );
                self.line(2, "if (v == 0) return \"0\";");
                self.line(2, "bool neg = v < 0;");
                self.line(2, "uint256 u = neg ? uint256(-v) : uint256(v);");
                self.line(2, "bytes memory b = new bytes(78);");
                self.line(2, "uint256 i = 78;");
                self.line(
                    2,
                    "while (u > 0) { i--; b[i] = bytes1(uint8(48 + u % 10)); u /= 10; }",
                );
                self.line(2, "bytes memory s = new bytes(78 - i + (neg ? 1 : 0));");
                self.line(2, "if (neg) s[0] = \"-\";");
                self.line(
                    2,
                    "for (uint256 j = 0; j < 78 - i; j++) s[j + (neg ? 1 : 0)] = b[i + j];",
                );
                self.line(2, "return string(s);");
                self.line(1, "}");
                self.out.push('\n');
            }
            // _cuni_dec_str helper if any dec say / interpolation needs it.
            // Canonical dec rendering (docs/DECIMAL.md §6); reuses _cuni_itoa
            // for the integer part (emitted above when needs_itoa).
            if self.needs_dec_str {
                self.line(
                    1,
                    "/// @notice scaled int256 -> canonical decimal string (docs/DECIMAL.md §6).",
                );
                self.line(
                    1,
                    "function _cuni_dec_str(int256 v) internal pure returns (string memory) {",
                );
                self.line(2, "bool neg = v < 0;");
                self.line(2, "uint256 mag = neg ? uint256(-(v + 1)) + 1 : uint256(v);");
                self.line(2, "uint256 ip = mag / 10000;");
                self.line(2, "uint256 fp = mag % 10000;");
                self.line(2, "bytes memory fb = new bytes(4);");
                self.line(2, "for (uint256 i = 0; i < 4; i++) {");
                self.line(3, "fb[3 - i] = bytes1(uint8(48 + (fp % 10)));");
                self.line(3, "fp /= 10;");
                self.line(2, "}");
                self.line(2, "uint256 flen = 4;");
                self.line(2, "while (flen > 1 && fb[flen - 1] == bytes1(uint8(48))) {");
                self.line(3, "unchecked { flen--; }");
                self.line(2, "}");
                self.line(2, "bytes memory frac = new bytes(flen);");
                self.line(2, "for (uint256 i = 0; i < flen; i++) { frac[i] = fb[i]; }");
                self.line(2, "string memory istr = _cuni_itoa(int256(ip));");
                self.line(2, "if (neg) {");
                self.line(3, "return string(abi.encodePacked(\"-\", istr, \".\", frac));");
                self.line(2, "}");
                self.line(2, "return string(abi.encodePacked(istr, \".\", frac));");
                self.line(1, "}");
                self.out.push('\n');
            }
            // _cuni_time_str (+ _cuni_time_padded) when a time is said or
            // interpolated. Canonical ISO-8601 UTC (docs/TIME.md §4); the
            // sol seat's uint256 epochs are non-negative by construction.
            if self.needs_time_str {
                self.line(
                    1,
                    "/// @notice zero-pad a component to at least `width` digits.",
                );
                self.line(
                    1,
                    "function _cuni_time_padded(uint256 x, uint256 width) internal pure returns (string memory) {",
                );
                self.line(2, "bytes memory b = new bytes(78);");
                self.line(2, "uint256 i = 78;");
                self.line(2, "if (x == 0) { i--; b[i] = \"0\"; }");
                self.line(
                    2,
                    "else { while (x > 0) { i--; b[i] = bytes1(uint8(48 + x % 10)); x /= 10; } }",
                );
                self.line(2, "uint256 len = 78 - i;");
                self.line(2, "uint256 pad = len < width ? width - len : 0;");
                self.line(2, "bytes memory s = new bytes(len + pad);");
                self.line(2, "for (uint256 j = 0; j < pad; j++) s[j] = \"0\";");
                self.line(2, "for (uint256 j = 0; j < len; j++) s[pad + j] = b[i + j];");
                self.line(2, "return string(s);");
                self.line(1, "}");
                self.out.push('\n');
                self.line(
                    1,
                    "/// @notice uint256 epoch -> canonical ISO-8601 UTC string (docs/TIME.md §4).",
                );
                self.line(
                    1,
                    "function _cuni_time_str(uint256 v) internal pure returns (string memory) {",
                );
                self.line(2, "uint256 ddays = v / 86400;  // `days` is a Solidity keyword");
                self.line(2, "uint256 sod = v % 86400;");
                self.line(2, "uint256 z = ddays + 719468;");
                self.line(2, "uint256 era = z / 146097;");
                self.line(2, "uint256 doe = z - era * 146097;");
                self.line(2, "uint256 yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;");
                self.line(2, "uint256 y = yoe + era * 400;");
                self.line(2, "uint256 doy = doe - (365 * yoe + yoe / 4 - yoe / 100);");
                self.line(2, "uint256 mp = (5 * doy + 2) / 153;");
                self.line(2, "uint256 d = doy - (153 * mp + 2) / 5 + 1;");
                self.line(2, "uint256 m = mp < 10 ? mp + 3 : mp - 9;");
                self.line(2, "if (m <= 2) y += 1;");
                self.line(2, "uint256 hh = sod / 3600;");
                self.line(2, "uint256 mi = (sod % 3600) / 60;");
                self.line(2, "uint256 ss = sod % 60;");
                self.line(
                    2,
                    "return string(abi.encodePacked(_cuni_time_padded(y, 4), \"-\", _cuni_time_padded(m, 2), \"-\", _cuni_time_padded(d, 2), \"T\", _cuni_time_padded(hh, 2), \":\", _cuni_time_padded(mi, 2), \":\", _cuni_time_padded(ss, 2), \"Z\"));",
                );
                self.line(1, "}");
                self.out.push('\n');
            }
            // Time arithmetic/parse helpers (docs/TIME.md §3, §5). Solidity
            // 0.8 checked arithmetic REVERTS on overflow — the loud refusal.
            if self.needs_time_helpers {
                self.line(
                    1,
                    "/// @notice strict ISO-8601 UTC -> uint256 epoch (docs/TIME.md §2, §5); reverts loudly on bad input.",
                );
                self.line(
                    1,
                    "function _cuni_time_num(bytes memory b, uint256 lo, uint256 n) internal pure returns (uint256) {",
                );
                self.line(2, "uint256 v = 0;");
                self.line(2, "for (uint256 i = 0; i < n; i++) {");
                self.line(3, "uint8 c = uint8(b[lo + i]);");
                self.line(3, "if (c < 48 || c > 57) revert(\"cuni: parse_time: bad ISO-8601 UTC timestamp\");");
                self.line(3, "v = v * 10 + (c - 48);");
                self.line(2, "}");
                self.line(2, "return v;");
                self.line(1, "}");
                self.out.push('\n');
                self.line(
                    1,
                    "function _cuni_parse_time(string memory s) internal pure returns (uint256) {",
                );
                self.line(2, "bytes memory b = bytes(s);");
                self.line(2, "if (b.length != 20) revert(\"cuni: parse_time: bad ISO-8601 UTC timestamp\");");
                self.line(2, "if (uint8(b[4]) != 45 || uint8(b[7]) != 45 || uint8(b[10]) != 84 || uint8(b[13]) != 58 || uint8(b[16]) != 58 || uint8(b[19]) != 90)");
                self.line(3, "revert(\"cuni: parse_time: bad ISO-8601 UTC timestamp\");");
                self.line(2, "uint256 y = _cuni_time_num(b, 0, 4);");
                self.line(2, "uint256 mo = _cuni_time_num(b, 5, 2);");
                self.line(2, "uint256 d = _cuni_time_num(b, 8, 2);");
                self.line(2, "uint256 h = _cuni_time_num(b, 11, 2);");
                self.line(2, "uint256 mi = _cuni_time_num(b, 14, 2);");
                self.line(2, "uint256 sec = _cuni_time_num(b, 17, 2);");
                self.line(2, "if (y < 1 || y > 9999 || mo < 1 || mo > 12) revert(\"cuni: parse_time: bad ISO-8601 UTC timestamp\");");
                self.line(2, "uint256 dim = 31;");
                self.line(2, "if (mo == 4 || mo == 6 || mo == 9 || mo == 11) dim = 30;");
                self.line(2, "else if (mo == 2) dim = (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) ? 29 : 28;");
                self.line(2, "if (d < 1 || d > dim || h > 23 || mi > 59 || sec > 59) revert(\"cuni: parse_time: bad ISO-8601 UTC timestamp\");");
                self.line(2, "uint256 y0 = mo <= 2 ? y - 1 : y;");
                self.line(2, "uint256 era = y0 / 400;");
                self.line(2, "uint256 yoe = y0 - era * 400;");
                self.line(2, "uint256 mp = (mo + 9) % 12;");
                self.line(2, "uint256 doy = (153 * mp + 2) / 5 + d - 1;");
                self.line(2, "uint256 doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;");
                self.line(2, "uint256 ddays = era * 146097 + doe - 719468;  // `days` is a Solidity keyword");
                self.line(2, "return ddays * 86400 + h * 3600 + mi * 60 + sec;");
                self.line(1, "}");
                self.out.push('\n');
                self.line(
                    1,
                    "/// @notice checked time + int seconds (docs/TIME.md §3); reverts on overflow.",
                );
                self.line(
                    1,
                    "function _cuni_add_seconds(uint256 t, int256 s) internal pure returns (uint256) {",
                );
                self.line(2, "if (s >= 0) return t + uint256(s);");
                self.line(2, "uint256 u = uint256(-(s + 1)) + 1;");
                self.line(2, "return t - u;");
                self.line(1, "}");
                self.out.push('\n');
                self.line(
                    1,
                    "/// @notice checked time - int seconds (docs/TIME.md §3); reverts on underflow.",
                );
                self.line(
                    1,
                    "function _cuni_sub_seconds(uint256 t, int256 s) internal pure returns (uint256) {",
                );
                self.line(2, "if (s >= 0) return t - uint256(s);");
                self.line(2, "uint256 u = uint256(-(s + 1)) + 1;");
                self.line(2, "return t + u;");
                self.line(1, "}");
                self.out.push('\n');
                self.line(
                    1,
                    "/// @notice whole days between two times, trunc toward zero (docs/TIME.md §5).",
                );
                self.line(
                    1,
                    "function _cuni_days_between(uint256 a, uint256 b) internal pure returns (int256) {",
                );
                self.line(2, "if (a >= b) return int256((a - b) / 86400);");
                self.line(2, "return -int256((b - a) / 86400);");
                self.line(1, "}");
                self.out.push('\n');
            }
    }

    fn gen_struct(&mut self, t: &TypDecl) -> Result<(), String> {
        let mut fields = Vec::new();
        for f in &t.fields {
            fields.push(format!("{} {};", sol_type(&f.ty)?, f.name));
        }
        self.line(0, &format!("struct {} {{", t.name));
        for f in fields {
            self.line(1, &f);
        }
        self.line(0, "}");
        self.out.push('\n');
        Ok(())
    }

    fn gen_enum(&mut self, e: &EnumDecl) {
        let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
        self.line(0, &format!("enum {} {{ {} }}", e.name, names.join(", ")));
        self.out.push('\n');
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        if f.fallible {
            // Fallible functions use revert; callers cannot try/catch internal
            // calls, so `??` on them is refused at the use site.
        }
        let mut params = Vec::new();
        for p in &f.params {
            let t = sol_type(&p.ty)?;
            let mem = if t == "string" { " memory" } else { "" };
            params.push(format!("{}{} {}", t, mem, p.name));
        }
        let ret = sol_type(&f.ret_type)?;
        let ret_mem = if ret == "string" { " memory" } else { "" };
        // A body that says anything is not pure.
        let pure = !body_uses_say(&f.body);
        let purity = if pure { " pure" } else { "" };
        self.line(
            1,
            &format!(
                "function {}({}) public{} returns ({}{}) {{",
                f.name,
                params.join(", "),
                purity,
                ret,
                ret_mem
            ),
        );
        let mut scope: HashMap<String, SolKind> = HashMap::new();
        for p in &f.params {
            scope.insert(p.name.clone(), kind_of_type(&p.ty));
        }
        for s in &f.body {
            self.gen_stmt(2, s, &mut scope)?;
        }
        self.line(1, "}");
        Ok(())
    }

    fn gen_stmt(
        &mut self,
        indent: usize,
        stmt: &Stmt,
        scope: &mut HashMap<String, SolKind>,
    ) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                if matches!(value.kind, ExprKind::Unwrap { .. }) {
                    return Err(
                        "`??` has no Solidity equivalent for internal calls; refusing".into(),
                    );
                }
                let kind = ty.as_ref().map(kind_of_type).unwrap_or(SolKind::Other);
                let kind = if kind == SolKind::Other {
                    // Infer from the value expression (literals and arithmetic).
                    self.expr_kind(value, scope)
                } else {
                    kind
                };
                // Declared type wins; otherwise infer from literal.
                let decl_ty = match ty {
                    Some(t) => sol_type(t)?,
                    None => match kind {
                        SolKind::Int | SolKind::Dec => "int256".into(),
                        SolKind::Time => "uint256".into(),
                        SolKind::Str => "string".into(),
                        SolKind::Bool => "bool".into(),
                        SolKind::Other => {
                            return Err(format!(
                                "cannot infer a Solidity type for `{}`; annotate it",
                                name
                            ))
                        }
                    },
                };
                // Memory annotation for strings.
                let mem = if decl_ty == "string" { " memory" } else { "" };
                scope.insert(name.clone(), kind);
                let v = self.gen_expr(value, scope)?;
                // String concat needs abi.encodePacked; gen_expr handles it.
                self.line(indent, &format!("{}{} {} = {};", decl_ty, mem, name, v));
            }
            StmtKind::Assign { target, value } => {
                let t = self.gen_expr(target, scope)?;
                let v = self.gen_expr(value, scope)?;
                self.line(indent, &format!("{} = {};", t, v));
            }
            StmtKind::Ret(Some(e)) => {
                let text = self.gen_expr(e, scope)?;
                self.line(indent, &format!("return {};", text));
            }
            StmtKind::Ret(None) => self.line(indent, "return;"),
            StmtKind::Fail(e) => match &e.kind {
                ExprKind::Str(s) => {
                    self.line(indent, &format!("revert(\"{}\");", Self::esc(s)));
                }
                _ => {
                    return Err(
                        "fail with a non-string has no clean Solidity revert; refusing".into(),
                    )
                }
            },
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("if ({}) {{", cond_s));
                self.gen_block(indent + 1, then_body, scope)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "} else {");
                    self.gen_block(indent + 1, else_body, scope)?;
                }
                self.line(indent, "}");
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                if b.is_some() {
                    return Err("two-binding for has no Solidity form; refusing".into());
                }
                // Only range() iteration is supported.
                let (start_s, end_e) = match &iter.kind {
                    ExprKind::Call { callee, args } => {
                        let is_range = matches!(&callee.kind, ExprKind::Ident(n) if n == "range");
                        if !is_range {
                            return Err(
                                "for over non-range iterables has no Solidity form; refusing"
                                    .into(),
                            );
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => {
                                return Err(
                                    "range() with step has no Solidity form; refusing".into()
                                )
                            }
                        }
                    }
                    _ => {
                        return Err(
                            "for over non-range iterables has no Solidity form; refusing".into(),
                        )
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr(se, scope)?,
                    None => "0".to_string(),
                };
                let e = self.gen_expr(end_e, scope)?;
                scope.insert(a.clone(), SolKind::Int);
                self.line(
                    indent,
                    &format!("for (int256 {} = {}; {} < {}; {}++) {{", a, s, a, e, a),
                );
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "}");
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("while ({}) {{", cond_s));
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "}");
            }
            StmtKind::ExprStmt(e) => {
                self.gen_expr_stmt(indent, e, scope)?;
            }
            StmtKind::Todo => {
                return Err("`...` placeholder body cannot become a contract; refusing".into())
            }
        }
        Ok(())
    }

    fn gen_block(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, SolKind>,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt(indent, s, scope)?;
        }
        Ok(())
    }

    /// Expression statements: only `say(...)` calls are meaningful at the top
    /// level of a contract; anything else is a no-op pure call.
    fn gen_expr_stmt(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &mut HashMap<String, SolKind>,
    ) -> Result<(), String> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                if vals.len() != 1 {
                    return Err("say takes exactly one argument".into());
                }
                let v = vals[0];
                let kind = self.expr_kind(v, scope);
                let text = self.gen_expr(v, scope)?;
                self.uses_emit = true;
                match kind {
                    SolKind::Int => self.line(indent, &format!("emit LogInt({});", text)),
                    SolKind::Dec => {
                        self.needs_dec_str = true;
                        self.line(
                            indent,
                            &format!("emit LogDec(_cuni_dec_str({}));", text),
                        )
                    }
                    SolKind::Time => {
                        self.needs_time_str = true;
                        self.line(
                            indent,
                            &format!("emit LogTime(_cuni_time_str({}));", text),
                        )
                    }
                    SolKind::Str => self.line(indent, &format!("emit LogString({});", text)),
                    SolKind::Bool => self.line(indent, &format!("emit LogBool({});", text)),
                    SolKind::Other => {
                        return Err("say of this value has no event mapping; refusing".into())
                    }
                }
                return Ok(());
            }
        }
        // A bare call with no effect: emit it so internal pure calls still run.
        let text = self.gen_expr(e, scope)?;
        self.line(indent, &format!("{};", text));
        Ok(())
    }

    fn gen_expr(&mut self, e: &Expr, scope: &HashMap<String, SolKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(n.to_string()),
            // Scaled int256 literal (docs/DECIMAL.md §2); the parser
            // validated i128 range, so int256 holds it exactly.
            ExprKind::Dec(s) => Ok(format!("int256({})", s)),
            // Epoch seconds (docs/TIME.md §2): non-negative (checked
            // above); uint256 literal.
            ExprKind::Time(e) => {
                if *e < 0 {
                    return Err(format!(
                        "sol seat: negative time epoch {e} cannot be a uint256 — refusing (docs/TIME.md §7)"
                    ));
                }
                Ok(format!("uint256({e})"))
            }
            ExprKind::Float(_) => Err("float literals have no Solidity form; refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(parts) => {
                let mut items = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => items.push(format!("\"{}\"", Self::esc(t))),
                        StrPartExpr::Expr(ie) => {
                            let k = self.expr_kind(ie, scope);
                            let t = self.gen_expr(ie, scope)?;
                            items.push(match k {
                                SolKind::Int => {
                                    self.needs_itoa = true;
                                    format!("_cuni_itoa({})", t)
                                }
                                SolKind::Dec => {
                                    self.needs_dec_str = true;
                                    format!("_cuni_dec_str({})", t)
                                }
                                SolKind::Time => {
                                    self.needs_time_str = true;
                                    format!("_cuni_time_str({})", t)
                                }
                                SolKind::Bool => {
                                    format!("({} ? \"true\" : \"false\")", t)
                                }
                                SolKind::Str => t,
                                SolKind::Other => {
                                    return Err(
                                        "interpolating this value has no Solidity form; refusing"
                                            .into(),
                                    )
                                }
                            });
                        }
                    }
                }
                Ok(format!("string(abi.encodePacked({}))", items.join(", ")))
            }
            ExprKind::NoneLit => Err("None has no Solidity form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Solidity form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 Solidity form; refusing".into()),
            ExprKind::Field { base, name } => {
                let b = self.gen_expr(base, scope)?;
                // Enum access Color.Green works as-is in Solidity.
                Ok(format!("{}.{}", b, name))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs, scope)?;
                let r = self.gen_expr(rhs, scope)?;
                let lk = self.expr_kind(lhs, scope);
                let rk = self.expr_kind(rhs, scope);
                // String + is concatenation.
                if matches!(*op, BinOp::Add) && lk == SolKind::Str {
                    return Ok(format!("string(abi.encodePacked({}, {}))", l, r));
                }
                // Solidity has no == on strings; compare via keccak256 hash.
                if matches!(*op, BinOp::Eq) && lk == SolKind::Str {
                    return Ok(format!(
                        "(keccak256(abi.encodePacked({})) == keccak256(abi.encodePacked({})))",
                        l, r
                    ));
                }
                if matches!(*op, BinOp::Ne) && lk == SolKind::Str {
                    return Ok(format!(
                        "(keccak256(abi.encodePacked({})) != keccak256(abi.encodePacked({})))",
                        l, r
                    ));
                }
                // `time` is a closed world (docs/TIME.md §3–5): the typeck
                // proved the valid shapes. Solidity 0.8 checked arithmetic
                // REVERTS on overflow — the loud refusal. `time` is uint256,
                // CuNi `int` is int256, so the int side is converted
                // explicitly via the checked helpers.
                if lk == SolKind::Time || rk == SolKind::Time {
                    let code = match op {
                        BinOp::Add if lk == SolKind::Time => {
                            self.needs_time_helpers = true;
                            format!("_cuni_add_seconds({l}, {r})")
                        }
                        BinOp::Add => {
                            self.needs_time_helpers = true;
                            format!("_cuni_add_seconds({r}, {l})")
                        }
                        BinOp::Sub if lk == SolKind::Time && rk == SolKind::Time => {
                            // time - time -> int256 seconds.
                            format!("(int256({l}) - int256({r}))")
                        }
                        BinOp::Sub if lk == SolKind::Time => {
                            self.needs_time_helpers = true;
                            format!("_cuni_sub_seconds({l}, {r})")
                        }
                        BinOp::Eq => format!("({l} == {r})"),
                        BinOp::Ne => format!("({l} != {r})"),
                        BinOp::Lt => format!("({l} < {r})"),
                        BinOp::Gt => format!("({l} > {r})"),
                        BinOp::Le => format!("({l} <= {r})"),
                        BinOp::Ge => format!("({l} >= {r})"),
                        _ => {
                            return Err("this operator is not defined on `time`; refusing".into())
                        }
                    };
                    return Ok(code);
                }
                // `dec` is a closed world (docs/DECIMAL.md §3–5): both
                // operands dec, or a loud refusal. The typeck already
                // rejected mixes; this is defense in depth. Solidity 0.8
                // checked arithmetic REVERTS on overflow, and `/` truncates
                // toward zero natively — both are the honest refusals.
                if lk == SolKind::Dec {
                    let code = match op {
                        BinOp::Add => format!("({l} + {r})"),
                        BinOp::Sub => format!("({l} - {r})"),
                        BinOp::Mul => format!("(({l}) * ({r}) / 10000)"),
                        BinOp::Div => format!("(({l}) * 10000 / ({r}))"),
                        BinOp::Mod => {
                            return Err("`%` is not defined on `dec`; refusing".into())
                        }
                        BinOp::Eq => format!("({l} == {r})"),
                        BinOp::Ne => format!("({l} != {r})"),
                        BinOp::Lt => format!("({l} < {r})"),
                        BinOp::Gt => format!("({l} > {r})"),
                        BinOp::Le => format!("({l} <= {r})"),
                        BinOp::Ge => format!("({l} >= {r})"),
                        BinOp::And | BinOp::Or => {
                            return Err("`and`/`or` need booleans; refusing".into())
                        }
                    };
                    return Ok(code);
                }
                let o = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    BinOp::Ge => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                // Solidity treats `/` on integer literals as rational division;
                // force int256 context so `7 / 2` is truncating integer division.
                if matches!(op, BinOp::Div | BinOp::Mod) {
                    return Ok(format!("(int256({}) {} int256({}))", l, o, r));
                }
                Ok(format!("({} {} {})", l, o, r))
            }
            ExprKind::Unary { op, expr } => {
                let t = self.gen_expr(expr, scope)?;
                match op {
                    UnOp::Not => Ok(format!("(!{})", t)),
                    UnOp::Neg => Ok(format!("(-{})", t)),
                }
            }
            ExprKind::Unwrap { .. } => {
                Err("`??` has no Solidity equivalent for internal calls; refusing".into())
            }
        }
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, SolKind>,
    ) -> Result<String, String> {
        // Wave-1 stdlib (docs/STDLIB.md §5): the sol seat refuses every
        // wave-1 function. Namespaced calls are refused here with the
        // documented reason; method calls (`.split`, `.trim`, …) fall
        // through to the direct-call refusal below.
        if let ExprKind::Field { base, name } = &callee.kind {
            if let ExprKind::Ident(ns) = &base.kind {
                if ns == "json" {
                    return Err(format!(
                        "`json.{}` has no Solidity form (no meaningful on-chain JSON); refusing",
                        name
                    ));
                }
                if ns == "time" {
                    return Err(format!(
                        "`time.{}` has no Solidity form (block timestamps are not calendar arithmetic); refusing",
                        name
                    ));
                }
            }
        }
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a Solidity form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no Solidity form; refusing".into());
        }
        // Builtins.
        match name.as_str() {
            "say" => return Err("say is a statement, not an expression; refusing".into()),
            "range" => return Err("range() outside for has no Solidity form; refusing".into()),
            // `dec` explicit conversions (docs/DECIMAL.md §5). Solidity 0.8
            // checked arithmetic reverts on overflow — the loud refusal.
            "dec_of_int" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("dec_of_int takes one argument".into());
                }
                return Ok(format!("(({} * 10000))", vals[0]));
            }
            "int_of_dec" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("int_of_dec takes one argument".into());
                }
                // `/` truncates toward zero natively.
                return Ok(format!("(({} / 10000))", vals[0]));
            }
            // `time` builtins (docs/TIME.md §5). Checked arithmetic reverts
            // on overflow — the loud refusal.
            "parse_time" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("parse_time takes one argument".into());
                }
                self.needs_time_helpers = true;
                return Ok(format!("(_cuni_parse_time({}))", vals[0]));
            }
            "add_seconds" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 2 {
                    return Err("add_seconds takes two arguments".into());
                }
                self.needs_time_helpers = true;
                return Ok(format!("(_cuni_add_seconds({}, {}))", vals[0], vals[1]));
            }
            "days_between" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 2 {
                    return Err("days_between takes two arguments".into());
                }
                self.needs_time_helpers = true;
                return Ok(format!("(_cuni_days_between({}, {}))", vals[0], vals[1]));
            }
            "len" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("len takes one argument".into());
                }
                let k = self.expr_kind(args[0].expr(), scope);
                return match k {
                    SolKind::Str => Ok(format!("(int256(bytes({}).length))", vals[0])),
                    _ => Err("len() of this value has no Solidity form; refusing".into()),
                };
            }
            "abs" | "min" | "max" => {
                return Err(format!(
                    "builtin `{}` needs a v1 helper; refusing for now",
                    name
                ))
            }
            // Wave-1 stdlib (docs/STDLIB.md §5).
            "sha256" => {
                return Err("`sha256` has no Solidity form (keccak256 ≠ SHA-256; the SHA-256 precompile is unobservable in the compile-only seat); refusing".into())
            }
            _ => {}
        }
        // Struct construction: Circle(2) or Circle(r=2) — positional only here.
        if self.typ_names.contains(&name) {
            let vals: Vec<String> = args
                .iter()
                .map(|a| self.gen_expr(a.expr(), scope))
                .collect::<Result<_, _>>()?;
            return Ok(format!("{}({})", name, vals.join(", ")));
        }
        if !self.fn_names.contains(&name) {
            return Err(format!("unknown call `{}`; refusing", name));
        }
        let vals: Vec<String> = args
            .iter()
            .map(|a| self.gen_expr(a.expr(), scope))
            .collect::<Result<_, _>>()?;
        Ok(format!("{}({})", name, vals.join(", ")))
    }

    /// Best-effort kind of an expression for event routing.
    fn expr_kind(&self, e: &Expr, scope: &HashMap<String, SolKind>) -> SolKind {
        match &e.kind {
            ExprKind::Int(_) => SolKind::Int,
            ExprKind::Dec(_) => SolKind::Dec,
            ExprKind::Str(_) | ExprKind::InterpStr(_) => SolKind::Str,
            ExprKind::Bool(_) => SolKind::Bool,
            ExprKind::Ident(n) => scope.get(n).copied().unwrap_or(SolKind::Other),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "len" => SolKind::Int,
                ExprKind::Ident(n) if n == "dec_of_int" => SolKind::Dec,
                ExprKind::Ident(n) if n == "int_of_dec" => SolKind::Int,
                ExprKind::Ident(n) if n == "parse_time" => SolKind::Time,
                ExprKind::Ident(n) if n == "add_seconds" => SolKind::Time,
                ExprKind::Ident(n) if n == "days_between" => SolKind::Int,
                ExprKind::Ident(n) => self.fn_ret.get(n).copied().unwrap_or(SolKind::Other),
                _ => SolKind::Other,
            },
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => SolKind::Bool,
                BinOp::Add if self.expr_kind(lhs, scope) == SolKind::Str => SolKind::Str,
                // time - time -> int (seconds); every other time shape -> time.
                BinOp::Sub
                    if self.expr_kind(lhs, scope) == SolKind::Time
                        && self.expr_kind(rhs, scope) == SolKind::Time =>
                {
                    SolKind::Int
                }
                _ => match self.expr_kind(lhs, scope) {
                    SolKind::Dec => SolKind::Dec,
                    SolKind::Time => SolKind::Time,
                    _ => match self.expr_kind(rhs, scope) {
                        SolKind::Time => SolKind::Time,
                        _ => SolKind::Int,
                    },
                },
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => SolKind::Bool,
                // Neg keeps the operand's kind (int stays int, dec stays dec).
                UnOp::Neg => self.expr_kind(expr, scope),
            },
            _ => SolKind::Other,
        }
    }
}

fn body_uses_say(body: &[Stmt]) -> bool {
    body.iter().any(stmt_uses_say)
}

fn stmt_uses_say(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::ExprStmt(e) => expr_uses_say(e),
        StmtKind::If {
            then_body,
            else_body,
            ..
        } => {
            body_uses_say(then_body)
                || else_body
                    .as_ref()
                    .map(|b| body_uses_say(b))
                    .unwrap_or(false)
        }
        StmtKind::For { body, .. } | StmtKind::Whl { body, .. } => body_uses_say(body),
        _ => false,
    }
}

fn expr_uses_say(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call { callee, args } => {
            (matches!(&callee.kind, ExprKind::Ident(n) if n == "say"))
                || args.iter().any(|a| expr_uses_say(a.expr()))
        }
        ExprKind::Binary { lhs, rhs, .. } => expr_uses_say(lhs) || expr_uses_say(rhs),
        ExprKind::Unary { expr, .. } => expr_uses_say(expr),
        ExprKind::InterpStr(parts) => parts.iter().any(|p| match p {
            StrPartExpr::Expr(ie) => expr_uses_say(ie),
            _ => false,
        }),
        _ => false,
    }
}
