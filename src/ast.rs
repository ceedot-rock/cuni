/// Byte offset range into the originating source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub fn union(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn dummy() -> Self {
        Span { start: 0, end: 0 }
    }
}

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
#[derive(Debug)]
pub struct EnumDecl {
    pub name: String,
    #[allow(dead_code)] // reserved for future enum-related diagnostics
    pub name_span: Span,
    pub variants: Vec<String>,
}

/// A non-portable, per-target binding: `ext name(...) -> T do py: ... go: ... end`.
#[derive(Debug)]
pub struct ExtDecl {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub ret_type: Type,
    pub targets: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct TypDecl {
    pub name: String,
    pub name_span: Span,
    pub implements: Option<String>,
    pub fields: Vec<Param>,
}

#[derive(Debug)]
pub struct IfaceDecl {
    pub name: String,
    pub name_span: Span,
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
    /// Span covering `name: type` (best-effort; starts at name).
    pub span: Span,
}

#[derive(Debug)]
pub struct FnDecl {
    pub name: String,
    pub name_span: Span,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret_type: Type,
    pub fallible: bool,
    pub body: Vec<Stmt>,
    pub is_link: bool,
}

#[derive(Debug, Clone)]
pub enum Type {
    Named(String),
    Generic(String, Vec<Type>), // list<T>, map<K,V>, opt<T>
}

#[derive(Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum StmtKind {
    Let {
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    Mut {
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    Ret(Option<Expr>),
    /// `fail expr` — signals failure from a fallible (`-> T ?`) function.
    Fail(Expr),
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    For {
        binding: (String, Option<String>),
        iter: Expr,
        body: Vec<Stmt>,
    },
    Whl {
        cond: Expr,
        body: Vec<Stmt>,
    },
    ExprStmt(Expr),
    /// The literal `...` placeholder used as a stand-in function body.
    Todo,
}

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Positional or named call argument. Named args are for typ constructors
/// (`Circle(r: 2.0)`); function calls stay positional in v0.1.x.
#[derive(Debug)]
pub enum CallArg {
    Pos(Expr),
    Named {
        name: String,
        name_span: Span,
        value: Expr,
    },
}

impl CallArg {
    pub fn expr(&self) -> &Expr {
        match self {
            CallArg::Pos(e) | CallArg::Named { value: e, .. } => e,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            CallArg::Pos(e) => e.span,
            CallArg::Named { name_span, value, .. } => name_span.union(value.span),
        }
    }

    pub fn is_named(&self) -> bool {
        matches!(self, CallArg::Named { .. })
    }
}

#[derive(Debug)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    InterpStr(Vec<StrPartExpr>),
    NoneLit,
    Ident(String),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Field {
        base: Box<Expr>,
        name: String,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    /// `expr ?? do ... end`
    Unwrap {
        expr: Box<Expr>,
        handler: Vec<Stmt>,
    },
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
