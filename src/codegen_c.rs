//! Native C seat. Tagged `Val` runtime covering the portable core.
//! Compiled with `gcc -x c`. This is a real C program, not a Python wrapper.

use crate::ast::*;
use std::collections::HashSet;

pub fn generate(program: &Program) -> String {
    let mut g = Gen {
        out: String::new(),
        fallible: HashSet::new(),
        typs: HashSet::new(),
        enums: Vec::new(),
        in_main: false,
    };
    for item in &program.items {
        if let Item::Def(f) = item {
            if f.fallible {
                g.fallible.insert(f.name.clone());
            }
        }
        if let Item::Typ(t) = item {
            g.typs.insert(t.name.clone());
        }
        if let Item::Enum(e) = item {
            g.enums.push((
                e.name.clone(),
                e.variants.iter().map(|v| v.name.clone()).collect(),
            ));
        }
    }
    g.out.push_str(CUNI_RT);
    g.out.push_str(CUNI_RT_STDLIB);
    g.out.push('\n');
    for (name, _) in &g.enums {
        g.out.push_str(&format!("static Val {name};\n"));
    }
    if !g.enums.is_empty() {
        g.out.push_str("static void cuni_enums_init(void) {\n");
        for (name, vars) in &g.enums {
            g.out
                .push_str(&format!("    {name} = V_struct(\"{name}\");\n"));
            for v in vars {
                g.out.push_str(&format!(
                    "    cuni_set(&{name}, \"{v}\", V_enum(\"{name}\", \"{v}\"));\n"
                ));
            }
        }
        g.out.push_str("}\n\n");
    }
    for item in &program.items {
        match item {
            Item::Typ(t) => g.typ(t),
            Item::Def(f) => g.func(f),
            Item::Ext(e) => {
                g.out.push_str(&format!(
                    "/* ext {} — no c: body; returns none */\n",
                    e.name
                ));
                let ps = (0..e.params.len())
                    .map(|i| format!("Val p{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ps = if ps.is_empty() { "void".into() } else { ps };
                g.out.push_str(&format!(
                    "static Val {}({}) {{ return V_none(); }}\n\n",
                    e.name, ps
                ));
            }
            _ => {}
        }
    }
    g.out.push_str("int main(void) {\n");
    g.out.push_str("    cuni_init();\n");
    g.in_main = true;
    if !g.enums.is_empty() {
        g.out.push_str("    cuni_enums_init();\n");
    }
    let mut script: Vec<&Stmt> = Vec::new();
    for item in &program.items {
        if let Item::Stmt(s) = item {
            script.push(s);
        }
    }
    for s in script {
        g.stmt(s, 1);
    }
    g.out.push_str("    return 0;\n}\n");
    g.out
}

struct Gen {
    out: String,
    fallible: HashSet<String>,
    typs: HashSet<String>,
    enums: Vec<(String, Vec<String>)>,
    in_main: bool,
}

impl Gen {
    fn typ(&mut self, t: &TypDecl) {
        let args = t
            .fields
            .iter()
            .enumerate()
            .map(|(i, _)| format!("Val a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.out
            .push_str(&format!("static Val {}({}) {{\n", t.name, args));
        self.out
            .push_str(&format!("    Val s = V_struct(\"{}\");\n", t.name));
        for (i, f) in t.fields.iter().enumerate() {
            self.out
                .push_str(&format!("    cuni_set(&s, \"{}\", a{i});\n", f.name));
        }
        self.out.push_str("    return s;\n}\n\n");
    }

    fn func(&mut self, f: &FnDecl) {
        let ps = f
            .params
            .iter()
            .map(|p| format!("Val {}", p.name))
            .collect::<Vec<_>>()
            .join(", ");
        let ps = if ps.is_empty() {
            "void".to_string()
        } else {
            ps
        };
        self.out
            .push_str(&format!("static Val {}({}) {{\n", f.name, ps));
        if f.body.is_empty() {
            self.out.push_str("    return V_none();\n}\n\n");
            return;
        }
        for s in &f.body {
            self.stmt(s, 1);
        }
        self.out.push_str("    return V_none();\n}\n\n");
    }

    fn stmt(&mut self, s: &Stmt, indent: usize) {
        let pad = "    ".repeat(indent);
        match &s.kind {
            StmtKind::Let { name, value, .. } | StmtKind::Mut { name, value, .. } => {
                if let ExprKind::Unwrap { expr, handler } = &value.kind {
                    let inner = self.expr(expr);
                    let fallible = matches!(&expr.kind, ExprKind::Call { callee, .. } if matches!(&callee.kind, ExprKind::Ident(n) if self.fallible.contains(n)));
                    if fallible {
                        self.out.push_str(&format!("{pad}cuni_failing = 0;\n"));
                        self.out.push_str(&format!("{pad}Val {name} = {inner};\n"));
                        self.out.push_str(&format!("{pad}if (cuni_failing) {{\n"));
                        self.out.push_str(&format!("{pad}    cuni_failing = 0;\n"));
                        for h in handler {
                            self.stmt(h, indent + 1);
                        }
                        self.out.push_str(&format!("{pad}}}\n"));
                    } else {
                        self.out.push_str(&format!("{pad}Val {name} = {inner};\n"));
                        self.out
                            .push_str(&format!("{pad}if ({name}.k == K_NONE) {{\n"));
                        for h in handler {
                            self.stmt(h, indent + 1);
                        }
                        self.out.push_str(&format!("{pad}}}\n"));
                    }
                } else {
                    let v = self.expr(value);
                    self.out.push_str(&format!("{pad}Val {name} = {v};\n"));
                }
            }
            StmtKind::Assign { target, value } => {
                let t = self.expr(target);
                let v = self.expr(value);
                self.out.push_str(&format!("{pad}{t} = {v};\n"));
            }
            StmtKind::Ret(Some(e)) => {
                if self.in_main {
                    self.out
                        .push_str(&format!("{pad}(void){};\n{pad}return 0;\n", self.expr(e)));
                } else {
                    self.out
                        .push_str(&format!("{pad}return {};\n", self.expr(e)));
                }
            }
            StmtKind::Ret(None) => {
                if self.in_main {
                    self.out.push_str(&format!("{pad}return 0;\n"));
                } else {
                    self.out.push_str(&format!("{pad}return V_none();\n"));
                }
            }
            StmtKind::Fail(e) => {
                self.out
                    .push_str(&format!("{pad}return fail_with({});\n", self.expr(e)));
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                self.out
                    .push_str(&format!("{pad}if (cuni_truthy({})) {{\n", self.expr(cond)));
                for s in then_body {
                    self.stmt(s, indent + 1);
                }
                if let Some(eb) = else_body {
                    self.out.push_str(&format!("{pad}}} else {{\n"));
                    for s in eb {
                        self.stmt(s, indent + 1);
                    }
                }
                self.out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                let it = self.expr(iter);
                self.out.push_str(&format!("{pad}{{\n"));
                self.out.push_str(&format!("{pad}    Val __it = {it};\n"));
                self.out.push_str(&format!(
                    "{pad}    for (size_t __i = 0; __i < __it.n; __i++) {{\n"
                ));
                if let Some(v) = &binding.1 {
                    self.out.push_str(&format!(
                        "{pad}        Val {} = V_int((long long)__i);\n",
                        binding.0
                    ));
                    self.out
                        .push_str(&format!("{pad}        Val {v} = __it.items[__i];\n"));
                } else {
                    self.out.push_str(&format!(
                        "{pad}        Val {} = __it.items[__i];\n",
                        binding.0
                    ));
                }
                for s in body {
                    self.stmt(s, indent + 2);
                }
                self.out.push_str(&format!("{pad}    }}\n"));
                self.out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::Whl { cond, body } => {
                self.out.push_str(&format!(
                    "{pad}while (cuni_truthy({})) {{\n",
                    self.expr(cond)
                ));
                for s in body {
                    self.stmt(s, indent + 1);
                }
                self.out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::ExprStmt(e) => {
                if let ExprKind::Call { callee, args } = &e.kind {
                    if let ExprKind::Ident(name) = &callee.kind {
                        if name == "say" {
                            let a = if args.is_empty() {
                                "V_str(\"\")".into()
                            } else {
                                self.expr(args[0].expr())
                            };
                            self.out.push_str(&format!("{pad}cuni_say({a});\n"));
                            return;
                        }
                    }
                }
                self.out
                    .push_str(&format!("{pad}(void){};\n", self.expr(e)));
            }
            StmtKind::Todo => self
                .out
                .push_str(&format!("{pad}return fail_with(V_str(\"...\"));\n")),
        }
    }

    fn expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => format!("V_int({n}LL)"),
            ExprKind::Dec(s) => {
                // Small scaled values fit a C constant; huge ones (up to
                // i128) go through the decimal-string parser — a 39-digit C
                // integer constant is not reliably accepted.
                if *s <= i64::MAX as i128 && *s >= i64::MIN as i128 {
                    format!("V_dec({s}LL)")
                } else {
                    format!("V_dec(cuni_dec_parse(\"{s}\"))")
                }
            }
            // Epoch seconds always fit int64 by construction (docs/TIME.md §2).
            ExprKind::Time(e) => format!("V_time({e}LL)"),
            ExprKind::Float(f) => format!("V_float({f:?})"),
            ExprKind::Bool(true) => "V_bool(1)".into(),
            ExprKind::Bool(false) => "V_bool(0)".into(),
            ExprKind::Str(s) => format!("V_str({})", c_string(s)),
            ExprKind::NoneLit => "V_none()".into(),
            ExprKind::Ident(s) => s.clone(),
            ExprKind::List(xs) => {
                if xs.is_empty() {
                    "V_list(0)".into()
                } else {
                    let inner = xs
                        .iter()
                        .map(|x| self.expr(x))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "({{ Val __xs[] = {{ {inner} }}; cuni_list_build({}u, __xs); }})",
                        xs.len()
                    )
                }
            }
            ExprKind::Map(pairs) => {
                // Wave-1: real maps (docs/STDLIB.md). Statement expression
                // builds the map; nested literals shadow __m legally.
                let sets = pairs
                    .iter()
                    .map(|(k, v)| {
                        format!("cuni_map_set(&__m, {}, {})", self.expr(k), self.expr(v))
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("({{ Val __m = V_map(); {sets}; __m; }})")
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Field { base, name } = &callee.kind {
                    // Wave-1 stdlib namespaces (docs/STDLIB.md).
                    if let ExprKind::Ident(ns) = &base.kind {
                        if ns == "json" || ns == "time" {
                            let a = args
                                .iter()
                                .map(|x| self.expr(x.expr()))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let f = match (ns.as_str(), name.as_str()) {
                                ("json", "parse") => "cuni_json_parse",
                                ("json", "emit") => "cuni_json_emit",
                                ("time", "epoch") => "cuni_time_epoch",
                                ("time", "parts") => "cuni_time_parts",
                                _ => "cuni_stdlib_unknown",
                            };
                            return format!("{f}({a})");
                        }
                    }
                    if name == "push" {
                        return format!(
                            "cuni_push(&{}, {})",
                            self.expr(base),
                            args.iter()
                                .map(|a| self.expr(a.expr()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    if name == "len" {
                        return format!("cuni_len({})", self.expr(base));
                    }
                    if name == "slice" && args.len() == 2 {
                        return format!(
                            "cuni_slice({}, {}, {})",
                            self.expr(base),
                            self.expr(args[0].expr()),
                            self.expr(args[1].expr())
                        );
                    }
                    // Wave-1 string ops (docs/STDLIB.md §3).
                    if name == "split" && args.len() == 1 {
                        return format!(
                            "cuni_split({}, {})",
                            self.expr(base),
                            self.expr(args[0].expr())
                        );
                    }
                    if name == "join" && args.len() == 1 {
                        return format!(
                            "cuni_join({}, {})",
                            self.expr(base),
                            self.expr(args[0].expr())
                        );
                    }
                    if name == "trim" && args.is_empty() {
                        return format!("cuni_trim({})", self.expr(base));
                    }
                    if name == "contains" && args.len() == 1 {
                        return format!(
                            "cuni_contains({}, {})",
                            self.expr(base),
                            self.expr(args[0].expr())
                        );
                    }
                }
                if let ExprKind::Ident(n) = &callee.kind {
                    let mapped = match n.as_str() {
                        "range" => "cuni_range",
                        "abs" => "cuni_abs",
                        "min" => "cuni_min",
                        "max" => "cuni_max",
                        "dec_of_int" => "cuni_dec_of_int",
                        "int_of_dec" => "cuni_int_of_dec",
                        // `time` builtins (docs/TIME.md §5).
                        "parse_time" => "cuni_parse_time",
                        "add_seconds" => "cuni_add_seconds",
                        "days_between" => "cuni_days_between",
                        // Wave-1 stdlib (docs/STDLIB.md §4).
                        "sha256" => "cuni_sha256",                        _ => "",
                    };
                    if !mapped.is_empty() {
                        let a = args
                            .iter()
                            .map(|x| self.expr(x.expr()))
                            .collect::<Vec<_>>()
                            .join(", ");
                        return format!("{mapped}({a})");
                    }
                }
                let c = self.expr(callee);
                if let ExprKind::Ident(n) = &callee.kind {
                    if self.typs.contains(n) {
                        let a = args
                            .iter()
                            .map(|x| self.expr(x.expr()))
                            .collect::<Vec<_>>()
                            .join(", ");
                        return format!("{n}({a})");
                    }
                }
                let a = args
                    .iter()
                    .map(|x| self.expr(x.expr()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{c}({a})")
            }
            ExprKind::Index { base, index } => {
                format!("cuni_index({}, {})", self.expr(base), self.expr(index))
            }
            ExprKind::Field { base, name } => {
                format!("cuni_get({}, \"{}\")", self.expr(base), name)
            }
            ExprKind::InterpStr(parts) => {
                let mut pieces = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => pieces.push(format!("V_str({})", c_string(t))),
                        StrPartExpr::Expr(ex) => {
                            pieces.push(format!("cuni_to_str({})", self.expr(ex)))
                        }
                    }
                }
                if pieces.is_empty() {
                    "V_str(\"\")".into()
                } else {
                    let mut acc = pieces[0].clone();
                    for p in pieces.iter().skip(1) {
                        acc = format!("cuni_concat({acc}, {p})");
                    }
                    acc
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                match op {
                    BinOp::Add => format!("cuni_add({l}, {r})"),
                    BinOp::Sub => format!("cuni_sub({l}, {r})"),
                    BinOp::Mul => format!("cuni_mul({l}, {r})"),
                    BinOp::Div => format!("cuni_div({l}, {r})"),
                    BinOp::Mod => format!("cuni_mod({l}, {r})"),
                    BinOp::Eq => format!("V_bool(cuni_eq({l}, {r}))"),
                    BinOp::Ne => format!("V_bool(!cuni_eq({l}, {r}))"),
                    BinOp::Lt => format!("V_bool(cuni_cmp({l}, {r}) < 0)"),
                    BinOp::Gt => format!("V_bool(cuni_cmp({l}, {r}) > 0)"),
                    BinOp::Le => format!("V_bool(cuni_cmp({l}, {r}) <= 0)"),
                    BinOp::Ge => format!("V_bool(cuni_cmp({l}, {r}) >= 0)"),
                    BinOp::And => format!("V_bool(cuni_truthy({l}) && cuni_truthy({r}))"),
                    BinOp::Or => format!("V_bool(cuni_truthy({l}) || cuni_truthy({r}))"),
                }
            }
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => format!("V_bool(!cuni_truthy({}))", self.expr(expr)),
                UnOp::Neg => format!("cuni_neg({})", self.expr(expr)),
            },
            ExprKind::Unwrap { expr, .. } => self.expr(expr),
        }
    }
}

fn c_string(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

const CUNI_RT: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>


typedef enum { K_INT, K_DEC, K_FLOAT, K_STR, K_BOOL, K_NONE, K_LIST, K_STRUCT, K_ENUM,
               /* Wave-1 stdlib: real maps (docs/STDLIB.md). Reuses keys/items/n. */
               K_MAP,
               /* CuNi `time`: int64 unix epoch seconds, UTC (docs/TIME.md). */
               K_TIME } K;typedef struct Val Val;
struct Val {
    K k;
    long long i;
    __int128 d;
    long long t;
    double f;
    char *s;
    int b;
    Val *items;
    size_t n, cap;
    char *tag;
    char **keys;
    char *variant;
};

static int cuni_failing;
static Val cuni_err;

static Val V_none(void) { Val v; memset(&v, 0, sizeof v); v.k = K_NONE; return v; }
static Val V_int(long long x) { Val v = V_none(); v.k = K_INT; v.i = x; return v; }
/* CuNi `dec`: fixed-point decimal, scale 10^4, exact (docs/DECIMAL.md). */
#define CUNI_DEC_SCALE 10000
static const unsigned __int128 CUNI_U128_MAX = (unsigned __int128)-1;
#define CUNI_I128_MAX ((__int128)(CUNI_U128_MAX >> 1))
#define CUNI_I128_MIN (-CUNI_I128_MAX - 1)
static Val V_dec(__int128 x) { Val v = V_none(); v.k = K_DEC; v.d = x; return v; }
static void cuni_dec_refuse(const char *msg) {
    fprintf(stderr, "cuni: %s — refused\n", msg);
    exit(1);
}
static unsigned __int128 cuni_uabs128(__int128 x) {
    return x < 0 ? (unsigned __int128)(-(x + 1)) + 1 : (unsigned __int128)x;
}
/* A dec literal too large for a C constant is emitted via this parser
   (the CuNi parser already proved it fits i128). */
static __int128 cuni_dec_parse(const char *s) {
    __int128 v = 0;
    int neg = 0;
    if (*s == '-') { neg = 1; s++; }
    while (*s) { v = v * 10 + (*s - '0'); s++; }
    return neg ? -v : v;
}
static Val cuni_dec_add(Val a, Val b) {
    __int128 x = a.d, y = b.d;
    if ((y > 0 && x > CUNI_I128_MAX - y) || (y < 0 && x < CUNI_I128_MIN - y))
        cuni_dec_refuse("dec addition overflow");
    return V_dec(x + y);
}
static Val cuni_dec_sub(Val a, Val b) {
    __int128 x = a.d, y = b.d;
    if ((y < 0 && x > CUNI_I128_MAX + y) || (y > 0 && x < CUNI_I128_MIN + y))
        cuni_dec_refuse("dec subtraction overflow");
    return V_dec(x - y);
}
static Val cuni_dec_mul(Val a, Val b) {
    /* trunc(x*y/10000) toward zero */
    unsigned __int128 ux = cuni_uabs128(a.d), uy = cuni_uabs128(b.d);
    if (ux != 0 && uy != 0 && ux > CUNI_U128_MAX / uy)
        cuni_dec_refuse("dec multiplication overflow");
    unsigned __int128 q = (ux * uy) / CUNI_DEC_SCALE;
    if (q > (unsigned __int128)CUNI_I128_MAX)
        cuni_dec_refuse("dec multiplication overflow");
    int neg = (a.d < 0) != (b.d < 0);
    return V_dec(neg ? -(__int128)q : (__int128)q);
}
static Val cuni_dec_div(Val a, Val b) {
    /* trunc(x*10000/y) toward zero */
    if (b.d == 0) cuni_dec_refuse("dec division by zero");
    unsigned __int128 ux = cuni_uabs128(a.d), uy = cuni_uabs128(b.d);
    if (ux > CUNI_U128_MAX / CUNI_DEC_SCALE)
        cuni_dec_refuse("dec division intermediate overflow");
    unsigned __int128 q = (ux * CUNI_DEC_SCALE) / uy;
    int neg = (a.d < 0) != (b.d < 0);
    if (q > (unsigned __int128)CUNI_I128_MAX + (unsigned __int128)(neg ? 1 : 0))
        cuni_dec_refuse("dec division overflow");
    if (neg)
        return V_dec(q == (unsigned __int128)CUNI_I128_MAX + 1 ? CUNI_I128_MIN : -(__int128)q);
    return V_dec((__int128)q);
}
static Val cuni_dec_neg(Val a) {
    if (a.d == CUNI_I128_MIN) cuni_dec_refuse("dec negation overflow");
    return V_dec(-a.d);
}
/* CuNi `time`: int64 unix epoch seconds, UTC, exact (docs/TIME.md). */
#define CUNI_I64_MAX 9223372036854775807LL
#define CUNI_I64_MIN (-CUNI_I64_MAX - 1)
static Val V_time(long long x) { Val v = V_none(); v.k = K_TIME; v.t = x; return v; }
static void cuni_time_refuse(const char *msg) {
    fprintf(stderr, "cuni: %s — refused\n", msg);
    exit(1);
}
/* a is K_TIME, b is K_INT (the caller checked the shape). */
static Val cuni_time_add(Val a, Val b) {
    long long x = a.t, y = b.i;
    if ((y > 0 && x > CUNI_I64_MAX - y) || (y < 0 && x < CUNI_I64_MIN - y))
        cuni_time_refuse("time addition overflow");
    return V_time(x + y);
}
/* a is K_TIME, b is K_INT (the caller checked the shape). */
static Val cuni_time_sub(Val a, Val b) {
    long long x = a.t, y = b.i;
    if ((y < 0 && x > CUNI_I64_MAX + y) || (y > 0 && x < CUNI_I64_MIN + y))
        cuni_time_refuse("time subtraction overflow");
    return V_time(x - y);
}
/* time - time -> int seconds (docs/TIME.md §3); both K_TIME. */
static Val cuni_time_diff(Val a, Val b) {
    long long x = a.t, y = b.t;
    if ((y < 0 && x > CUNI_I64_MAX + y) || (y > 0 && x < CUNI_I64_MIN + y))
        cuni_time_refuse("time difference overflow");
    return V_int(x - y);
}
static Val cuni_time_neg(Val a) {
    if (a.t == CUNI_I64_MIN) cuni_time_refuse("time negation overflow");
    return V_time(-a.t);
}
/* Canonical ISO-8601 UTC rendering (docs/TIME.md §4) into `out` (>= 32 bytes). */
static void cuni_time_str_buf(long long e, char *out) {
    long long days = e / 86400, sod = e % 86400;
    if (sod < 0) { days--; sod += 86400; }
    long long z = days + 719468;
    long long era = z >= 0 ? z / 146097 : -((-z + 146096) / 146097);
    long long doe = z - era * 146097;
    long long yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    long long y = yoe + era * 400;
    long long doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    long long mp = (5 * doy + 2) / 153;
    long long d = doy - (153 * mp + 2) / 5 + 1;
    long long m = mp < 10 ? mp + 3 : mp - 9;
    if (m <= 2) y++;
    long long hh = sod / 3600, mi = (sod % 3600) / 60, ss = sod % 60;
    char *p = out;
    if (y < 0) { *p++ = '-'; y = -y; }
    char yb[24]; int yn = 0;
    if (y == 0) yb[yn++] = '0';
    else while (y > 0) { yb[yn++] = (char)('0' + y % 10); y /= 10; }
    while (yn < 4) yb[yn++] = '0';
    for (int i = yn - 1; i >= 0; i--) *p++ = yb[i];
    *p++ = '-';
    long long parts[5] = {m, d, hh, mi, ss};
    const char seps[5] = {'-', 'T', ':', ':', 'Z'};
    for (int k = 0; k < 5; k++) {
        *p++ = (char)('0' + parts[k] / 10);
        *p++ = (char)('0' + parts[k] % 10);
        *p++ = seps[k];
    }
    *p = 0;
}
static long long cuni_time_days_from_civil(long long y, long long m, long long d) {
    long long y0 = m <= 2 ? y - 1 : y;
    long long era = y0 >= 0 ? y0 / 400 : -((-y0 + 399) / 400);
    long long yoe = y0 - era * 400;
    long long mp = (m + 9) % 12;
    long long doy = (153 * mp + 2) / 5 + d - 1;
    long long doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + doe - 719468;
}
/* Strict ISO-8601 UTC -> time (docs/TIME.md §2, §5): bad input refuses loudly. */
static Val cuni_parse_time(Val s) {
    if (s.k != K_STR || !s.s) cuni_time_refuse("parse_time needs a string");
    const char *t = s.s;
    if (strlen(t) != 20) cuni_time_refuse("parse_time: bad ISO-8601 UTC timestamp");
    if (t[4] != '-' || t[7] != '-' || t[10] != 'T' || t[13] != ':' || t[16] != ':' || t[19] != 'Z')
        cuni_time_refuse("parse_time: bad ISO-8601 UTC timestamp");
    long long dg[6];
    const int pos[6][2] = {{0,4},{5,7},{8,10},{11,13},{14,16},{17,19}};
    for (int k = 0; k < 6; k++) {
        long long v = 0;
        for (int i = pos[k][0]; i < pos[k][1]; i++) {
            if (t[i] < '0' || t[i] > '9')
                cuni_time_refuse("parse_time: bad ISO-8601 UTC timestamp");
            v = v * 10 + (t[i] - '0');
        }
        dg[k] = v;
    }
    long long y = dg[0], mo = dg[1], d = dg[2], h = dg[3], mi = dg[4], sec = dg[5];
    if (y < 1 || y > 9999 || mo < 1 || mo > 12)
        cuni_time_refuse("parse_time: bad ISO-8601 UTC timestamp");
    long long dim = 31;
    if (mo == 4 || mo == 6 || mo == 9 || mo == 11) dim = 30;
    else if (mo == 2) dim = (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) ? 29 : 28;
    if (d < 1 || d > dim || h > 23 || mi > 59 || sec > 59)
        cuni_time_refuse("parse_time: bad ISO-8601 UTC timestamp");
    return V_time(cuni_time_days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec);
}
static Val cuni_add_seconds(Val t, Val s) {
    if (t.k != K_TIME || s.k != K_INT) cuni_time_refuse("add_seconds needs (time, int)");
    return cuni_time_add(t, s);
}
static Val cuni_days_between(Val a, Val b) {
    if (a.k != K_TIME || b.k != K_TIME) cuni_time_refuse("days_between needs (time, time)");
    /* Truncation toward zero (docs/TIME.md §5); C's / truncates natively. */
    long long x = a.t, y = b.t;
    if ((y < 0 && x > CUNI_I64_MAX + y) || (y > 0 && x < CUNI_I64_MIN + y))
        cuni_time_refuse("days_between overflow");
    return V_int((x - y) / 86400);
}
/* Canonical dec rendering (docs/DECIMAL.md §6) into `out` (>= 64 bytes). */
static void cuni_dec_str_buf(__int128 v, char *out) {
    int neg = v < 0;
    unsigned __int128 mag = cuni_uabs128(v);
    unsigned __int128 ip = mag / CUNI_DEC_SCALE;
    unsigned __int128 fp = mag % CUNI_DEC_SCALE;
    char tmp[64]; int n = 0;
    if (ip == 0) tmp[n++] = '0';
    else while (ip > 0) { tmp[n++] = '0' + (int)(ip % 10); ip /= 10; }
    char *p = out;
    if (neg) *p++ = '-';
    for (int i = n - 1; i >= 0; i--) *p++ = tmp[i];
    *p++ = '.';
    char fbuf[5];
    for (int i = 3; i >= 0; i--) { fbuf[i] = '0' + (int)(fp % 10); fp /= 10; }
    int flen = 4;
    while (flen > 1 && fbuf[flen - 1] == '0') flen--;
    memcpy(p, fbuf, flen); p += flen;
    *p = 0;
}
static Val cuni_dec_of_int(Val n) {
    if (n.k != K_INT) cuni_dec_refuse("dec_of_int needs an int");
    return V_dec((__int128)n.i * CUNI_DEC_SCALE);
}
static Val cuni_int_of_dec(Val d) {
    /* truncates toward zero (docs/DECIMAL.md §5) */
    if (d.k != K_DEC) cuni_dec_refuse("int_of_dec needs a dec");
    __int128 q = d.d / CUNI_DEC_SCALE;
    if (q > (__int128)9223372036854775807LL || q < (__int128)(-9223372036854775807LL - 1))
        cuni_dec_refuse("int_of_dec out of int range");
    return V_int((long long)q);
}
static Val V_float(double x) { Val v = V_none(); v.k = K_FLOAT; v.f = x; return v; }
static Val V_bool(int x) { Val v = V_none(); v.k = K_BOOL; v.b = x ? 1 : 0; return v; }
static Val V_str(const char *s) {
    Val v = V_none(); v.k = K_STR;
    v.s = strdup(s ? s : "");
    return v;
}
static Val V_enum(const char *ty, const char *var) {
    Val v = V_none(); v.k = K_ENUM;
    v.tag = strdup(ty); v.variant = strdup(var);
    return v;
}
static Val V_struct(const char *name) {
    Val v = V_none(); v.k = K_STRUCT;
    v.tag = strdup(name);
    return v;
}
static Val V_list(size_t cap) {
    Val v = V_none(); v.k = K_LIST;
    if (cap) { v.items = (Val*)calloc(cap, sizeof(Val)); v.cap = cap; }
    return v;
}
static Val cuni_list_build(size_t n, Val *xs) {
    Val v = V_list(n ? n : 1);
    v.n = n;
    for (size_t i = 0; i < n; i++) v.items[i] = xs[i];
    return v;
}

static void cuni_init(void) { cuni_failing = 0; }

static Val fail_with(Val e) { cuni_failing = 1; cuni_err = e; return V_none(); }

static void cuni_set(Val *s, const char *key, Val val) {
    s->keys = (char**)realloc(s->keys, (s->n + 1) * sizeof(char*));
    s->items = (Val*)realloc(s->items, (s->n + 1) * sizeof(Val));
    s->keys[s->n] = strdup(key);
    s->items[s->n] = val;
    s->n++;
}
static Val cuni_get(Val s, const char *key) {
    for (size_t i = 0; i < s.n; i++) {
        if (s.keys && s.keys[i] && strcmp(s.keys[i], key) == 0) return s.items[i];
    }
    if (s.k == K_ENUM && s.variant && strcmp(s.variant, key) == 0) return s;
    return V_none();
}
static Val cuni_index(Val s, Val idx) {
    long long i = idx.i;
    if (s.k == K_LIST && i >= 0 && (size_t)i < s.n) return s.items[i];
    return V_none();
}
static Val cuni_len(Val s) {
    if (s.k == K_STR && s.s) return V_int((long long)strlen(s.s));
    return V_int((long long)s.n);
}
static Val cuni_range(Val n) {
    long long m = n.i;
    if (m <= 0) return V_list(0);
    Val v = V_list((size_t)m);
    v.n = (size_t)m;
    for (long long i = 0; i < m; i++) v.items[i] = V_int(i);
    return v;
}
static Val cuni_abs(Val n) { long long v = n.i; return V_int(v < 0 ? -v : v); }
static Val cuni_min(Val a, Val b) { return V_int(a.i <= b.i ? a.i : b.i); }
static Val cuni_max(Val a, Val b) { return V_int(a.i >= b.i ? a.i : b.i); }
static Val cuni_slice(Val s, Val a, Val b) {
    long long ia = a.i, ib = b.i;
    if (s.k == K_STR && s.s) {
        long long n = (long long)strlen(s.s);
        if (ia < 0 || ib < 0 || ia > n || ib > n || ia > ib) return V_str("");
        size_t len = (size_t)(ib - ia);
        char *p = (char*)malloc(len + 1);
        memcpy(p, s.s + (size_t)ia, len);
        p[len] = 0;
        Val v = V_none(); v.k = K_STR; v.s = p; return v;
    }
    if (s.k == K_LIST) {
        long long n = (long long)s.n;
        if (ia < 0 || ib < 0 || ia > n || ib > n || ia > ib) return V_list(0);
        Val v = V_list((size_t)(ib - ia));
        v.n = (size_t)(ib - ia);
        for (size_t i = 0; i < v.n; i++) v.items[i] = s.items[(size_t)ia + i];
        return v;
    }
    return V_none();
}
static Val cuni_push(Val *s, Val x) {
    if (s->n + 1 > s->cap) {
        s->cap = s->cap ? s->cap * 2 : 4;
        s->items = (Val*)realloc(s->items, s->cap * sizeof(Val));
    }
    s->items[s->n++] = x;
    s->k = K_LIST;
    return V_none();
}
static int cuni_eq(Val a, Val b) {
    if (a.k != b.k) return 0;
    switch (a.k) {
        case K_INT: return a.i == b.i;
        case K_DEC: return a.d == b.d;
        case K_TIME: return a.t == b.t;
        case K_FLOAT: return a.f == b.f;
        case K_BOOL: return a.b == b.b;
        case K_STR: return a.s && b.s && strcmp(a.s, b.s) == 0;
        case K_NONE: return 1;
        case K_ENUM: return a.variant && b.variant && strcmp(a.variant, b.variant) == 0;
        default: return 0;
    }
}
static int cuni_truthy(Val a) {
    switch (a.k) {
        case K_NONE: return 0;
        case K_BOOL: return a.b;
        case K_INT: return a.i != 0;
        case K_DEC: return a.d != 0;
        case K_TIME: return a.t != 0;
        case K_FLOAT: return a.f != 0;
        case K_STR: return a.s && a.s[0];
        default: return 1;
    }
}
static int cuni_cmp(Val a, Val b) {
    /* `dec` compares scaled integers directly — never via double. */
    if (a.k == K_DEC && b.k == K_DEC) {
        if (a.d < b.d) return -1;
        if (a.d > b.d) return 1;
        return 0;
    }
    if (a.k == K_DEC || b.k == K_DEC)
        cuni_dec_refuse("cannot mix dec and non-dec — convert explicitly");
    /* `time` compares epoch integers directly (docs/TIME.md §3). */
    if (a.k == K_TIME && b.k == K_TIME) {
        if (a.t < b.t) return -1;
        if (a.t > b.t) return 1;
        return 0;
    }
    if (a.k == K_TIME || b.k == K_TIME)
        cuni_time_refuse("cannot mix time and non-time — durations are plain int seconds (docs/TIME.md §3)");
    double x = (a.k == K_FLOAT) ? a.f : (double)a.i;
    double y = (b.k == K_FLOAT) ? b.f : (double)b.i;
    if (x < y) return -1;
    if (x > y) return 1;
    return 0;
}
static double as_f(Val a) { return a.k == K_FLOAT ? a.f : (double)a.i; }
/* `dec` is a closed world (docs/DECIMAL.md §3–5): both operands dec, or a
   loud refusal. The typeck already rejected mixes; this is defense in depth. */
static void cuni_dec_check_pair(Val a, Val b) {
    if (a.k != K_DEC || b.k != K_DEC)
        cuni_dec_refuse("cannot mix dec and non-dec — convert explicitly (`dec_of_int` / `int_of_dec`)");
}
static Val cuni_add(Val a, Val b) {
    if (a.k == K_DEC || b.k == K_DEC) { cuni_dec_check_pair(a, b); return cuni_dec_add(a, b); }
    /* `time` is a closed world (docs/TIME.md §3): (time,int)/(int,time) ->
       time. The typeck proved the shape; this is defense in depth. */
    if (a.k == K_TIME || b.k == K_TIME) {
        if (a.k == K_TIME && b.k == K_INT) return cuni_time_add(a, b);
        if (a.k == K_INT && b.k == K_TIME) return cuni_time_add(b, a);
        cuni_time_refuse("cannot add time to this operand — durations are plain int seconds (docs/TIME.md §3)");
    }
    if (a.k == K_STR || b.k == K_STR) {
        /* handled by concat path for strings of numbers too */
    }
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) + as_f(b));
    return V_int(a.i + b.i);
}
static Val cuni_sub(Val a, Val b) {
    if (a.k == K_DEC || b.k == K_DEC) { cuni_dec_check_pair(a, b); return cuni_dec_sub(a, b); }
    /* `time - int -> time`, `time - time -> int` (docs/TIME.md §3). */
    if (a.k == K_TIME || b.k == K_TIME) {
        if (a.k == K_TIME && b.k == K_INT) return cuni_time_sub(a, b);
        if (a.k == K_TIME && b.k == K_TIME) return cuni_time_diff(a, b);
        cuni_time_refuse("cannot subtract this from/to a time — `time - int -> time`, `time - time -> int` (docs/TIME.md §3)");
    }
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) - as_f(b));
    return V_int(a.i - b.i);
}
static Val cuni_mul(Val a, Val b) {
    if (a.k == K_DEC || b.k == K_DEC) { cuni_dec_check_pair(a, b); return cuni_dec_mul(a, b); }
    if (a.k == K_TIME || b.k == K_TIME) cuni_time_refuse("`*` is not defined on `time` (docs/TIME.md §3)");
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) * as_f(b));
    return V_int(a.i * b.i);
}
static Val cuni_div(Val a, Val b) {
    if (a.k == K_DEC || b.k == K_DEC) { cuni_dec_check_pair(a, b); return cuni_dec_div(a, b); }
    if (a.k == K_TIME || b.k == K_TIME) cuni_time_refuse("`/` is not defined on `time` (docs/TIME.md §3)");
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) / as_f(b));
    return V_int(b.i == 0 ? 0 : a.i / b.i);
}
static Val cuni_mod(Val a, Val b) {
    if (a.k == K_DEC || b.k == K_DEC) cuni_dec_refuse("`%` is not defined on `dec`");
    if (a.k == K_TIME || b.k == K_TIME) cuni_time_refuse("`%` is not defined on `time` (docs/TIME.md §3)");
    return V_int(b.i == 0 ? 0 : a.i % b.i);
}
static Val cuni_neg(Val a) {
    if (a.k == K_DEC) return cuni_dec_neg(a);
    if (a.k == K_TIME) return cuni_time_neg(a);
    if (a.k == K_FLOAT) return V_float(-a.f);
    return V_int(-a.i);
}
static Val cuni_to_str(Val a) {
    char buf[128];
    switch (a.k) {
        case K_INT: snprintf(buf, sizeof buf, "%lld", a.i); return V_str(buf);
        case K_DEC: cuni_dec_str_buf(a.d, buf); return V_str(buf);
        case K_TIME: cuni_time_str_buf(a.t, buf); return V_str(buf);
        case K_FLOAT: snprintf(buf, sizeof buf, "%.15g", a.f); return V_str(buf);
        case K_STR: return a;
        case K_BOOL: return V_str(a.b ? "true" : "false");
        case K_NONE: return V_str("none");
        default: return V_str("");
    }
}
static Val cuni_concat(Val a, Val b) {
    Val sa = cuni_to_str(a), sb = cuni_to_str(b);
    size_t n = strlen(sa.s) + strlen(sb.s);
    char *p = (char*)malloc(n + 1);
    strcpy(p, sa.s); strcat(p, sb.s);
    Val r = V_str(p); free(p); return r;
}
static void cuni_say(Val v) {
    switch (v.k) {
        case K_INT: printf("%lld\n", v.i); break;
        case K_DEC: { char buf[128]; cuni_dec_str_buf(v.d, buf); printf("%s\n", buf); break; }
        case K_TIME: { char buf[64]; cuni_time_str_buf(v.t, buf); printf("%s\n", buf); break; }
        case K_FLOAT: printf("%.15g\n", v.f); break;
        case K_STR: printf("%s\n", v.s ? v.s : ""); break;
        case K_BOOL: printf("%s\n", v.b ? "True" : "False"); break;
        case K_NONE: printf("None\n"); break;
        default: printf("%s\n", cuni_to_str(v).s); break;
    }
}
"#;

/// Wave-1 stdlib runtime (docs/STDLIB.md). Hand-rolled C99: no dependencies
/// beyond libc. Mirrors the spec algorithms exactly.
const CUNI_RT_STDLIB: &str = r#"
/* ================= Wave-1 stdlib (docs/STDLIB.md) ================= */
static void cuni_panic(const char *msg) {
    fprintf(stderr, "cuni: %s\n", msg);
    exit(1);
}
static Val V_map(void) { Val v = V_none(); v.k = K_MAP; return v; }
/* Duplicate keys: last wins, first position kept (docs/STDLIB.md §1.2). */
static void cuni_map_set(Val *m, Val key, Val val) {
    if (key.k != K_STR || !key.s) cuni_panic("map keys must be strings");
    for (size_t i = 0; i < m->n; i++) {
        if (m->keys && m->keys[i] && strcmp(m->keys[i], key.s) == 0) {
            m->items[i] = val;
            return;
        }
    }
    cuni_set(m, key.s, val);
}
static Val cuni_strn(const char *s, size_t len) {
    char *p = (char*)malloc(len + 1);
    if (!p) cuni_panic("out of memory");
    memcpy(p, s, len);
    p[len] = 0;
    Val v = V_none(); v.k = K_STR; v.s = p;
    return v;
}
/* Growable byte buffer. */
typedef struct { char *p; size_t n, cap; } CuniCBuf;
static void cuni_cbuf_reserve(CuniCBuf *b, size_t extra) {
    if (b->n + extra + 1 > b->cap) {
        size_t nc = b->cap ? b->cap * 2 : 64;
        while (b->n + extra + 1 > nc) nc *= 2;
        b->p = (char*)realloc(b->p, nc);
        if (!b->p) cuni_panic("out of memory");
        b->cap = nc;
    }
}
static void cuni_cbuf_str(CuniCBuf *b, const char *s, size_t len) {
    cuni_cbuf_reserve(b, len);
    memcpy(b->p + b->n, s, len);
    b->n += len;
    b->p[b->n] = 0;
}
static void cuni_cbuf_c(CuniCBuf *b, char c) { cuni_cbuf_str(b, &c, 1); }
static void cuni_cbuf_cstr(CuniCBuf *b, const char *s) { cuni_cbuf_str(b, s, strlen(s)); }

/* ---------- JSON (docs/STDLIB.md §1) ---------- */
typedef struct { const char *s; size_t pos; } CuniJPar;
static void cuni_j_ws(CuniJPar *p) {
    while (p->s[p->pos]==' '||p->s[p->pos]=='\t'||p->s[p->pos]=='\n'||p->s[p->pos]=='\r') p->pos++;
}
static Val cuni_j_value(CuniJPar *p);
static void cuni_j_utf8(CuniCBuf *b, unsigned cp) {
    if (cp < 0x80) { cuni_cbuf_c(b, (char)cp); }
    else if (cp < 0x800) { cuni_cbuf_c(b, (char)(0xC0 | (cp >> 6))); cuni_cbuf_c(b, (char)(0x80 | (cp & 63))); }
    else if (cp < 0x10000) {
        cuni_cbuf_c(b, (char)(0xE0 | (cp >> 12)));
        cuni_cbuf_c(b, (char)(0x80 | ((cp >> 6) & 63)));
        cuni_cbuf_c(b, (char)(0x80 | (cp & 63)));
    } else {
        cuni_cbuf_c(b, (char)(0xF0 | (cp >> 18)));
        cuni_cbuf_c(b, (char)(0x80 | ((cp >> 12) & 63)));
        cuni_cbuf_c(b, (char)(0x80 | ((cp >> 6) & 63)));
        cuni_cbuf_c(b, (char)(0x80 | (cp & 63)));
    }
}
static unsigned cuni_j_hex4(CuniJPar *p) {
    unsigned v = 0;
    for (int i = 0; i < 4; i++) {
        char c = p->s[p->pos++];
        v <<= 4;
        if (c >= '0' && c <= '9') v |= (unsigned)(c - '0');
        else if (c >= 'a' && c <= 'f') v |= (unsigned)(c - 'a' + 10);
        else if (c >= 'A' && c <= 'F') v |= (unsigned)(c - 'A' + 10);
        else cuni_panic("json.parse: bad \\u escape");
    }
    return v;
}
static char *cuni_j_string(CuniJPar *p) {
    p->pos++; /* opening " */
    CuniCBuf b; b.p = 0; b.n = 0; b.cap = 0;
    for (;;) {
        size_t start = p->pos;
        while (p->s[p->pos] && p->s[p->pos] != '"' && p->s[p->pos] != '\\') p->pos++;
        for (size_t i = start; i < p->pos; i++) {
            if ((unsigned char)p->s[i] < 0x20) cuni_panic("json.parse: unescaped control character in string");
        }
        cuni_cbuf_str(&b, p->s + start, p->pos - start);
        char c = p->s[p->pos];
        if (!c) cuni_panic("json.parse: unterminated string");
        if (c == '"') { p->pos++; return b.p ? b.p : strdup(""); }
        p->pos++; /* backslash */
        char e = p->s[p->pos++];
        switch (e) {
            case '"': cuni_cbuf_c(&b, '"'); break;
            case '\\': cuni_cbuf_c(&b, '\\'); break;
            case '/': cuni_cbuf_c(&b, '/'); break;
            case 'b': cuni_cbuf_c(&b, '\b'); break;
            case 'f': cuni_cbuf_c(&b, '\f'); break;
            case 'n': cuni_cbuf_c(&b, '\n'); break;
            case 'r': cuni_cbuf_c(&b, '\r'); break;
            case 't': cuni_cbuf_c(&b, '\t'); break;
            case 'u': {
                unsigned hi = cuni_j_hex4(p);
                if (hi >= 0xD800 && hi < 0xDC00) {
                    if (p->s[p->pos] != '\\' || p->s[p->pos+1] != 'u') cuni_panic("json.parse: lone surrogate");
                    p->pos += 2;
                    unsigned lo = cuni_j_hex4(p);
                    if (lo < 0xDC00 || lo >= 0xE000) cuni_panic("json.parse: lone surrogate");
                    cuni_j_utf8(&b, 0x10000u + ((hi - 0xD800u) << 10) + (lo - 0xDC00u));
                } else if (hi >= 0xDC00 && hi < 0xE000) {
                    cuni_panic("json.parse: lone surrogate");
                } else {
                    cuni_j_utf8(&b, hi);
                }
                break;
            }
            default: cuni_panic("json.parse: bad escape");
        }
    }
}
/* Value-based integer rule (docs/STDLIB.md §1.1). */
static long long cuni_j_number(CuniJPar *p) {
    const char *s = p->s;
    size_t i = p->pos;
    int neg = 0;
    size_t int_s, int_e, frac_s = 0, frac_e = 0;
    if (s[i] == '-') { neg = 1; i++; }
    int_s = i;
    if (s[i] == '0') { i++; }
    else if (s[i] >= '1' && s[i] <= '9') { while (s[i] >= '0' && s[i] <= '9') i++; }
    else cuni_panic("json.parse: bad number");
    int_e = i;
    if (s[i] == '.') {
        i++;
        frac_s = i;
        while (s[i] >= '0' && s[i] <= '9') i++;
        frac_e = i;
        if (frac_e == frac_s) cuni_panic("json.parse: bad number");
    }
    long long exp = 0;
    if (s[i] == 'e' || s[i] == 'E') {
        i++;
        int eneg = 0;
        if (s[i] == '-') { eneg = 1; i++; }
        else if (s[i] == '+') { i++; }
        size_t es = i;
        while (s[i] >= '0' && s[i] <= '9') i++;
        if (i == es) cuni_panic("json.parse: bad number");
        if (i - es > 18) cuni_panic("json.parse: number is not an integer in ±(2^53−1)");
        char ebuf[20];
        memcpy(ebuf, s + es, i - es);
        ebuf[i - es] = 0;
        exp = strtoll(ebuf, 0, 10);
        if (eneg) exp = -exp;
    }
    p->pos = i;
    /* significant digits: int part + frac part, leading zeros stripped */
    size_t nd = (int_e - int_s) + (frac_e - frac_s);
    if (nd > 32) cuni_panic("json.parse: number is not an integer in ±(2^53−1)");
    char dig[33];
    size_t w = 0;
    for (size_t j = int_s; j < int_e; j++) dig[w++] = s[j];
    for (size_t j = frac_s; j < frac_e; j++) dig[w++] = s[j];
    size_t nz = 0;
    while (nz < w && dig[nz] == '0') nz++;
    if (nz == w) return 0;
    if (w - nz > 16) cuni_panic("json.parse: number is not an integer in ±(2^53−1)");
    long long d = 0;
    for (size_t j = nz; j < w; j++) d = d * 10 + (dig[j] - '0');
    long long f = (long long)(frac_e - frac_s);
    while (d % 10 == 0 && d != 0) { d /= 10; f--; }
    long long k = f - exp;
    long long v;
    if (k <= 0) {
        v = d;
        for (long long q = 0; q < -k; q++) {
            if (v > 9007199254740991LL/10 || v < -9007199254740991LL/10)
                cuni_panic("json.parse: number is not an integer in ±(2^53−1)");
            v *= 10;
        }
    } else {
        if (k > 16) cuni_panic("json.parse: number is not an integer in ±(2^53−1)");
        long long p10 = 1;
        for (long long q = 0; q < k; q++) p10 *= 10;
        if (d % p10 != 0) cuni_panic("json.parse: number is not an integer in ±(2^53−1)");
        v = d / p10;
    }
    if (neg) v = -v;
    if (v < -9007199254740991LL || v > 9007199254740991LL)
        cuni_panic("json.parse: number is not an integer in ±(2^53−1)");
    return v;
}
static Val cuni_j_object(CuniJPar *p) {
    p->pos++; /* { */
    Val m = V_map();
    cuni_j_ws(p);
    if (p->s[p->pos] == '}') { p->pos++; return m; }
    for (;;) {
        cuni_j_ws(p);
        if (p->s[p->pos] != '"') cuni_panic("json.parse: object keys must be strings");
        char *key = cuni_j_string(p);
        cuni_j_ws(p);
        if (p->s[p->pos] != ':') { free(key); cuni_panic("json.parse: expected ':'"); }
        p->pos++; cuni_j_ws(p);
        Val v = cuni_j_value(p);
        Val kk = V_str(key); free(key);
        cuni_map_set(&m, kk, v);
        cuni_j_ws(p);
        if (p->s[p->pos] == ',') { p->pos++; continue; }
        if (p->s[p->pos] == '}') { p->pos++; return m; }
        cuni_panic("json.parse: expected ',' or '}'");
    }
}
static Val cuni_j_array(CuniJPar *p) {
    p->pos++; /* [ */
    Val xs = V_list(4);
    cuni_j_ws(p);
    if (p->s[p->pos] == ']') { p->pos++; return xs; }
    for (;;) {
        cuni_j_ws(p);
        cuni_push(&xs, cuni_j_value(p));
        cuni_j_ws(p);
        if (p->s[p->pos] == ',') { p->pos++; continue; }
        if (p->s[p->pos] == ']') { p->pos++; return xs; }
        cuni_panic("json.parse: expected ',' or ']'");
    }
}
static Val cuni_j_value(CuniJPar *p) {
    char c = p->s[p->pos];
    switch (c) {
        case '{': return cuni_j_object(p);
        case '[': return cuni_j_array(p);
        case '"': { char *t = cuni_j_string(p); Val v = V_str(t); free(t); return v; }
        case 't':
            if (!strncmp(p->s + p->pos, "true", 4)) { p->pos += 4; return V_bool(1); }
            break;
        case 'f':
            if (!strncmp(p->s + p->pos, "false", 5)) { p->pos += 5; return V_bool(0); }
            break;
        case 'n':
            if (!strncmp(p->s + p->pos, "null", 4)) { p->pos += 4; return V_none(); }
            break;
        case '-': case '0': case '1': case '2': case '3': case '4':
        case '5': case '6': case '7': case '8': case '9':
            return V_int(cuni_j_number(p));
        default: break;
    }
    cuni_panic("json.parse: unexpected character");
    return V_none();
}
static Val cuni_json_parse(Val s) {
    if (s.k != K_STR) cuni_panic("json.parse needs a str");
    CuniJPar p; p.s = s.s ? s.s : ""; p.pos = 0;
    cuni_j_ws(&p);
    Val v = cuni_j_value(&p);
    cuni_j_ws(&p);
    if (p.s[p.pos]) cuni_panic("json.parse: trailing characters");
    if (v.k != K_MAP) cuni_panic("json.parse: top-level JSON value must be an object");
    return v;
}
/* Canonical minimal emit (docs/STDLIB.md §1.3). */
static void cuni_j_write_str(const char *s, CuniCBuf *out) {
    cuni_cbuf_c(out, '"');
    const unsigned char *p = (const unsigned char *)s;
    while (*p) {
        unsigned cp;
        size_t len;
        if (*p < 0x80) { cp = *p; len = 1; }
        else if ((*p >> 5) == 6) { cp = *p & 0x1F; len = 2; }
        else if ((*p >> 4) == 14) { cp = *p & 0x0F; len = 3; }
        else { cp = *p & 0x07; len = 4; }
        for (size_t i = 1; i < len && p[i]; i++) cp = (cp << 6) | (p[i] & 0x3F);
        switch (cp) {
            case '"': cuni_cbuf_cstr(out, "\\\""); break;
            case '\\': cuni_cbuf_cstr(out, "\\\\"); break;
            case 0x08: cuni_cbuf_cstr(out, "\\b"); break;
            case 0x0C: cuni_cbuf_cstr(out, "\\f"); break;
            case 0x0A: cuni_cbuf_cstr(out, "\\n"); break;
            case 0x0D: cuni_cbuf_cstr(out, "\\r"); break;
            case 0x09: cuni_cbuf_cstr(out, "\\t"); break;
            default:
                if (cp < 0x20) {
                    char esc[8];
                    snprintf(esc, sizeof esc, "\\u%04x", cp);
                    cuni_cbuf_cstr(out, esc);
                } else {
                    cuni_cbuf_str(out, (const char*)p, len);
                }
        }
        p += len;
    }
    cuni_cbuf_c(out, '"');
}
static void cuni_j_write(Val v, CuniCBuf *out) {
    char tmp[32];
    size_t i;
    switch (v.k) {
        case K_INT:
            snprintf(tmp, sizeof tmp, "%lld", v.i);
            cuni_cbuf_cstr(out, tmp);
            break;
        case K_STR: cuni_j_write_str(v.s ? v.s : "", out); break;
        case K_BOOL: cuni_cbuf_cstr(out, v.b ? "true" : "false"); break;
        case K_NONE: cuni_cbuf_cstr(out, "null"); break;
        case K_LIST:
            cuni_cbuf_c(out, '[');
            for (i = 0; i < v.n; i++) {
                if (i) cuni_cbuf_c(out, ',');
                cuni_j_write(v.items[i], out);
            }
            cuni_cbuf_c(out, ']');
            break;
        case K_MAP: {
            /* keys sorted in byte order */
            size_t *idx = (size_t*)malloc(v.n * sizeof(size_t));
            if (!idx && v.n) cuni_panic("out of memory");
            for (i = 0; i < v.n; i++) idx[i] = i;
            for (i = 1; i < v.n; i++) {
                size_t t = idx[i], j = i;
                while (j > 0 && strcmp(v.keys[idx[j-1]], v.keys[t]) > 0) {
                    idx[j] = idx[j-1];
                    j--;
                }
                idx[j] = t;
            }
            cuni_cbuf_c(out, '{');
            for (i = 0; i < v.n; i++) {
                if (i) cuni_cbuf_c(out, ',');
                cuni_j_write_str(v.keys[idx[i]], out);
                cuni_cbuf_c(out, ':');
                cuni_j_write(v.items[idx[i]], out);
            }
            cuni_cbuf_c(out, '}');
            free(idx);
            break;
        }
        default: cuni_panic("json.emit: value has no JSON form");
    }
}
static Val cuni_json_emit(Val m) {
    if (m.k != K_MAP) cuni_panic("json.emit needs a map");
    CuniCBuf b; b.p = 0; b.n = 0; b.cap = 0;
    cuni_j_write(m, &b);
    Val r = V_str(b.p ? b.p : "");
    free(b.p);
    return r;
}

/* ---------- time (docs/STDLIB.md §2) ---------- */
static long long cuni_days_from_civil(long long y, long long m, long long d) {
    long long y0 = m <= 2 ? y - 1 : y;
    long long era = y0 / 400;
    long long yoe = y0 - era * 400;
    long long mp = (m + 9) % 12;
    long long doy = (153 * mp + 2) / 5 + d - 1;
    long long doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + doe - 719468;
}
static void cuni_civil_from_days(long long z, long long *y, long long *m, long long *d) {
    z += 719468;
    long long era = z / 146097;
    long long doe = z - era * 146097;
    long long yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    long long yy = yoe + era * 400;
    long long doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    long long mp = (5 * doy + 2) / 153;
    long long dd = doy - (153 * mp + 2) / 5 + 1;
    long long mm = mp < 10 ? mp + 3 : mp - 9;
    *y = mm <= 2 ? yy + 1 : yy;
    *m = mm;
    *d = dd;
}
/* floor division for b > 0 */
static long long cuni_fdiv_ll(long long a, long long b) {
    long long q = a / b, r = a % b;
    return r < 0 ? q - 1 : q;
}
static Val cuni_time_epoch(Val y, Val mo, Val d, Val h, Val mi, Val s) {
    long long yy = y.i, mm = mo.i, dd = d.i, hh = h.i, mmi = mi.i, ss = s.i;
    if (yy < 1 || yy > 9999) cuni_panic("time.epoch: year out of range 1..9999");
    if (mm < 1 || mm > 12) cuni_panic("time.epoch: month out of range 1..12");
    long long dim = 31;
    if (mm == 4 || mm == 6 || mm == 9 || mm == 11) dim = 30;
    else if (mm == 2) dim = (yy % 4 == 0 && (yy % 100 != 0 || yy % 400 == 0)) ? 29 : 28;
    if (dd < 1 || dd > dim) cuni_panic("time.epoch: day out of range for month");
    if (hh < 0 || hh > 23) cuni_panic("time.epoch: hour out of range 0..23");
    if (mmi < 0 || mmi > 59) cuni_panic("time.epoch: minute out of range 0..59");
    if (ss < 0 || ss > 59) cuni_panic("time.epoch: second out of range 0..59");
    return V_int(cuni_days_from_civil(yy, mm, dd) * 86400 + hh * 3600 + mmi * 60 + ss);
}
static Val cuni_time_parts(Val e) {
    long long ev = e.i;
    long long lo = cuni_days_from_civil(1, 1, 1) * 86400;
    long long hi = cuni_days_from_civil(9999, 12, 31) * 86400 + 86399;
    if (ev < lo || ev > hi) cuni_panic("time.parts: epoch out of range 1..9999");
    long long days = cuni_fdiv_ll(ev, 86400);
    long long secs = ev - days * 86400;
    long long y, mo, d;
    cuni_civil_from_days(days, &y, &mo, &d);
    Val m = V_map();
    Val t;
    t = V_int(y); cuni_map_set(&m, V_str("year"), t);
    t = V_int(mo); cuni_map_set(&m, V_str("month"), t);
    t = V_int(d); cuni_map_set(&m, V_str("day"), t);
    t = V_int(secs / 3600); cuni_map_set(&m, V_str("hour"), t);
    t = V_int((secs % 3600) / 60); cuni_map_set(&m, V_str("min"), t);
    t = V_int(secs % 60); cuni_map_set(&m, V_str("sec"), t);
    return m;
}

/* ---------- strings (docs/STDLIB.md §3): byte-oriented ---------- */
static Val cuni_split(Val s, Val sep) {
    if (s.k != K_STR || sep.k != K_STR) cuni_panic(".split needs strings");
    const char *ss = s.s ? s.s : "", *pp = sep.s ? sep.s : "";
    size_t plen = strlen(pp);
    if (plen == 0) cuni_panic(".split: empty separator; refusing");
    Val out = V_list(4);
    const char *start = ss;
    const char *hit;
    while ((hit = strstr(start, pp)) != 0) {
        cuni_push(&out, cuni_strn(start, (size_t)(hit - start)));
        start = hit + plen;
    }
    cuni_push(&out, cuni_strn(start, strlen(start)));
    return out;
}
static Val cuni_join(Val sep, Val parts) {
    if (sep.k != K_STR) cuni_panic(".join needs a str separator");
    if (parts.k != K_LIST) cuni_panic(".join needs a list<str>");
    const char *ss = sep.s ? sep.s : "";
    CuniCBuf b; b.p = 0; b.n = 0; b.cap = 0;
    for (size_t i = 0; i < parts.n; i++) {
        if (parts.items[i].k != K_STR) cuni_panic(".join: all parts must be str");
        if (i) cuni_cbuf_cstr(&b, ss);
        cuni_cbuf_cstr(&b, parts.items[i].s ? parts.items[i].s : "");
    }
    Val r = V_str(b.p ? b.p : "");
    free(b.p);
    return r;
}
static int cuni_is_trim_c(char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\v' || c == '\f' || c == '\r';
}
static Val cuni_trim(Val s) {
    if (s.k != K_STR) cuni_panic(".trim needs a str");
    const char *ss = s.s ? s.s : "";
    size_t n = strlen(ss), a = 0, b = n;
    while (a < b && cuni_is_trim_c(ss[a])) a++;
    while (b > a && cuni_is_trim_c(ss[b - 1])) b--;
    return cuni_strn(ss + a, b - a);
}
static Val cuni_contains(Val s, Val sub) {
    if (s.k != K_STR || sub.k != K_STR) cuni_panic(".contains needs strings");
    const char *ss = s.s ? s.s : "", *pp = sub.s ? sub.s : "";
    return V_bool(strstr(ss, pp) != 0);
}

/* ---------- SHA-256 (FIPS 180-4; docs/STDLIB.md §4) ---------- */
static unsigned cuni_ror32(unsigned x, int n) { return (x >> n) | (x << (32 - n)); }
static Val cuni_sha256(Val s) {
    if (s.k != K_STR) cuni_panic("sha256 needs a str");
    const unsigned char *msg0 = (const unsigned char *)(s.s ? s.s : "");
    size_t len0 = strlen((const char *)msg0);
    size_t nblocks = (len0 + 9 + 63) / 64;
    size_t n = nblocks * 64;
    unsigned char *msg = (unsigned char *)calloc(n, 1);
    if (!msg) cuni_panic("out of memory");
    memcpy(msg, msg0, len0);
    msg[len0] = 0x80;
    unsigned long long bitlen = (unsigned long long)len0 * 8;
    for (int i = 0; i < 8; i++) msg[n - 1 - i] = (unsigned char)(bitlen >> (8 * i));
    unsigned h[8] = {0x6a09e667u, 0xbb67ae85u, 0x3c6ef372u, 0xa54ff53au,
                     0x510e527fu, 0x9b05688cu, 0x1f83d9abu, 0x5be0cd19u};
    static const unsigned kk[64] = {
        0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,0x923f82a4u,0xab1c5ed5u,
        0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,
        0xe49b69c1u,0xefbe4786u,0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,
        0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,0x06ca6351u,0x14292967u,
        0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,
        0xa2bfe8a1u,0xa81a664bu,0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,
        0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,0x5b9cca4fu,0x682e6ff3u,
        0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u,
    };
    for (size_t b = 0; b < nblocks; b++) {
        unsigned w[64];
        for (int i = 0; i < 16; i++) {
            w[i] = ((unsigned)msg[b*64+4*i] << 24) | ((unsigned)msg[b*64+4*i+1] << 16) |
                   ((unsigned)msg[b*64+4*i+2] << 8) | (unsigned)msg[b*64+4*i+3];
        }
        for (int i = 16; i < 64; i++) {
            unsigned s0 = cuni_ror32(w[i-15], 7) ^ cuni_ror32(w[i-15], 18) ^ (w[i-15] >> 3);
            unsigned s1 = cuni_ror32(w[i-2], 17) ^ cuni_ror32(w[i-2], 19) ^ (w[i-2] >> 10);
            w[i] = w[i-16] + s0 + w[i-7] + s1;
        }
        unsigned a=h[0], bb=h[1], c=h[2], d=h[3], e=h[4], f=h[5], g=h[6], hh=h[7];
        for (int i = 0; i < 64; i++) {
            unsigned s1 = cuni_ror32(e, 6) ^ cuni_ror32(e, 11) ^ cuni_ror32(e, 25);
            unsigned ch = (e & f) ^ (~e & g);
            unsigned t1 = hh + s1 + ch + kk[i] + w[i];
            unsigned s0 = cuni_ror32(a, 2) ^ cuni_ror32(a, 13) ^ cuni_ror32(a, 22);
            unsigned maj = (a & bb) ^ (a & c) ^ (bb & c);
            unsigned t2 = s0 + maj;
            hh = g; g = f; f = e; e = d + t1;
            d = c; c = bb; bb = a; a = t1 + t2;
        }
        h[0]+=a; h[1]+=bb; h[2]+=c; h[3]+=d; h[4]+=e; h[5]+=f; h[6]+=g; h[7]+=hh;
    }
    free(msg);
    char hex[65];
    for (int i = 0; i < 8; i++) snprintf(hex + 8*i, 9, "%08x", h[i]);
    return V_str(hex);
}
"#;
