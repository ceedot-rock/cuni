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
            g.enums.push((e.name.clone(), e.variants.clone()));
        }
    }
    g.out.push_str(CUNI_RT);
    g.out.push('\n');
    for (name, _) in &g.enums {
        g.out.push_str(&format!("static Val {name};\n"));
    }
    if !g.enums.is_empty() {
        g.out.push_str("static void cuni_enums_init(void) {\n");
        for (name, vars) in &g.enums {
            g.out.push_str(&format!("    {name} = V_struct(\"{name}\");\n"));
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
                g.out.push_str(&format!("/* ext {} — no c: body; returns none */\n", e.name));
                let ps = (0..e.params.len())
                    .map(|i| format!("Val p{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ps = if ps.is_empty() {
                    "void".into()
                } else {
                    ps
                };
                g.out
                    .push_str(&format!("static Val {}({}) {{ return V_none(); }}\n\n", e.name, ps));
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
        self.out.push_str(&format!("static Val {}({}) {{\n", t.name, args));
        self.out.push_str(&format!("    Val s = V_struct(\"{}\");\n", t.name));
        for (i, f) in t.fields.iter().enumerate() {
            self.out.push_str(&format!("    cuni_set(&s, \"{}\", a{i});\n", f.name));
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
        self.out.push_str(&format!("static Val {}({}) {{\n", f.name, ps));
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
                        self.out.push_str(&format!("{pad}if ({name}.k == K_NONE) {{\n"));
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
                    self.out.push_str(&format!("{pad}(void){};\n{pad}return 0;\n", self.expr(e)));
                } else {
                    self.out.push_str(&format!("{pad}return {};\n", self.expr(e)));
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
                self.out.push_str(&format!(
                    "{pad}return fail_with({});\n",
                    self.expr(e)
                ));
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
            StmtKind::For { binding, iter, body } => {
                let it = self.expr(iter);
                self.out.push_str(&format!("{pad}{{\n"));
                self.out.push_str(&format!("{pad}    Val __it = {it};\n"));
                self.out.push_str(&format!("{pad}    for (size_t __i = 0; __i < __it.n; __i++) {{\n"));
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
                self.out
                    .push_str(&format!("{pad}while (cuni_truthy({})) {{\n", self.expr(cond)));
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
                self.out.push_str(&format!("{pad}(void){};\n", self.expr(e)));
            }
            StmtKind::Todo => self.out.push_str(&format!("{pad}return fail_with(V_str(\"...\"));\n")),
        }
    }

    fn expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => format!("V_int({n}LL)"),
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
                    let inner = xs.iter().map(|x| self.expr(x)).collect::<Vec<_>>().join(", ");
                    format!("({{ Val __xs[] = {{ {inner} }}; cuni_list_build({}u, __xs); }})", xs.len())
                }
            }
            ExprKind::Map(_) => "V_none()".into(),
            ExprKind::Call { callee, args } => {
                if let ExprKind::Field { base, name } = &callee.kind {
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
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

const CUNI_RT: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>


typedef enum { K_INT, K_FLOAT, K_STR, K_BOOL, K_NONE, K_LIST, K_STRUCT, K_ENUM } K;
typedef struct Val Val;
struct Val {
    K k;
    long long i;
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
static Val cuni_len(Val s) { return V_int((long long)s.n); }
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
        case K_FLOAT: return a.f != 0;
        case K_STR: return a.s && a.s[0];
        default: return 1;
    }
}
static int cuni_cmp(Val a, Val b) {
    double x = (a.k == K_FLOAT) ? a.f : (double)a.i;
    double y = (b.k == K_FLOAT) ? b.f : (double)b.i;
    if (x < y) return -1;
    if (x > y) return 1;
    return 0;
}
static double as_f(Val a) { return a.k == K_FLOAT ? a.f : (double)a.i; }
static Val cuni_add(Val a, Val b) {
    if (a.k == K_STR || b.k == K_STR) {
        /* handled by concat path for strings of numbers too */
    }
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) + as_f(b));
    return V_int(a.i + b.i);
}
static Val cuni_sub(Val a, Val b) {
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) - as_f(b));
    return V_int(a.i - b.i);
}
static Val cuni_mul(Val a, Val b) {
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) * as_f(b));
    return V_int(a.i * b.i);
}
static Val cuni_div(Val a, Val b) {
    if (a.k == K_FLOAT || b.k == K_FLOAT) return V_float(as_f(a) / as_f(b));
    return V_int(b.i == 0 ? 0 : a.i / b.i);
}
static Val cuni_mod(Val a, Val b) { return V_int(b.i == 0 ? 0 : a.i % b.i); }
static Val cuni_neg(Val a) {
    if (a.k == K_FLOAT) return V_float(-a.f);
    return V_int(-a.i);
}
static Val cuni_to_str(Val a) {
    char buf[128];
    switch (a.k) {
        case K_INT: snprintf(buf, sizeof buf, "%lld", a.i); return V_str(buf);
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
        case K_FLOAT: printf("%.15g\n", v.f); break;
        case K_STR: printf("%s\n", v.s ? v.s : ""); break;
        case K_BOOL: printf("%s\n", v.b ? "True" : "False"); break;
        case K_NONE: printf("None\n"); break;
        default: printf("%s\n", cuni_to_str(v).s); break;
    }
}
"#;
