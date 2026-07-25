#[derive(Debug)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Use(String),
    Ext(ExtDecl),
    Typ(TypDecl),
    Iface(IfaceDecl),
    Enum(EnumDecl),
    Def(FnDecl),
    Stmt(Stmt),
}

/// Payload-free enum: a closed set of named variants, no attached data.
/// Full tagged unions with payload are an open item (SPEC.md §16) — Go has no
/// honest way to make the compiler refuse an unsealed/non-exhaustive union.
#[derive(Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
}

/// A non-portable, per-target binding: `ext name(...) -> T do py: ... go: ... end`.
/// Each target line is captured as raw source text, not parsed as CuNi.
#[derive(Debug)]
pub struct ExtDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_type: Type,
    pub targets: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct TypDecl {
    pub name: String,
    pub implements: Option<String>,
    pub fields: Vec<Param>,
}

#[derive(Debug)]
pub struct IfaceDecl {
    pub name: String,
    pub methods: Vec<MethodSig>,
}

#[derive(Debug)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_type: Type,
}

#[derive(Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug)]
pub struct FnDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret_type: Type,
    pub fallible: bool,
    pub body: Vec<Stmt>,
    /// True for `link` declarations (SPEC.md §16) rather than plain `def`.
    /// A `link` is a `def` in every respect locally — same body, same
    /// fallibility rules — plus it additionally generates a wire handler and
    /// an always-fallible `<name>_remote` client stub per target, since a
    /// `link` is meant to be callable both in-process and over the network
    /// from a separately-compiled CuNi program. No generics: a wire contract
    /// needs a concrete, enumerable shape, which a type parameter doesn't
    /// give it.
    pub is_link: bool,
}

#[derive(Debug, Clone)]
pub enum Type {
    Named(String),
    Generic(String, Vec<Type>), // list<T>, map<K,V>, opt<T>
}

#[derive(Debug)]
pub enum Stmt {
    Let { name: String, ty: Option<Type>, value: Expr },
    Mut { name: String, ty: Option<Type>, value: Expr },
    Assign { target: Expr, value: Expr },
    Ret(Option<Expr>),
    /// `fail expr` — signals failure from a fallible (`-> T ?`) function
    /// body, symmetric with `ret`: `ret` succeeds with a value, `fail` fails
    /// with one. Each target picks its own idiomatic failure channel
    /// (Python exception, Go error return, JS throw) — see codegen docs.
    Fail(Expr),
    If { cond: Expr, then_body: Vec<Stmt>, else_body: Option<Vec<Stmt>> },
    For { binding: (String, Option<String>), iter: Expr, body: Vec<Stmt> },
    Whl { cond: Expr, body: Vec<Stmt> },
    ExprStmt(Expr),
    /// The literal `...` placeholder used as a stand-in function body.
    Todo,
}

#[derive(Debug)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    InterpStr(Vec<StrPartExpr>),
    NoneLit,
    Ident(String),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Index { base: Box<Expr>, index: Box<Expr> },
    Field { base: Box<Expr>, name: String },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary { op: UnOp, expr: Box<Expr> },
    /// `expr ?? do ... end` — unwraps a fallible result or an `opt<T>`,
    /// running `handler` (which must `ret`) when there's nothing to unwrap.
    Unwrap { expr: Box<Expr>, handler: Vec<Stmt> },
}

#[derive(Debug)]
pub enum StrPartExpr {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Not,
    Neg,
}
