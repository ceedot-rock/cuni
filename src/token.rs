#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // literals
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    InterpStr(Vec<StrPart>),
    True,
    False,
    NoneLit,

    // keywords
    Do,
    End,
    Let,
    Mut,
    Def,
    Link,
    Ret,
    Fail,
    Enum,
    If,
    Els,
    For,
    In,
    Whl,
    Typ,
    Iface,
    Is,
    Use,
    Ext,
    And,
    Or,
    Not,

    // symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Lt,
    Gt,
    Comma,
    Colon,
    Dot,
    Arrow,
    Question,
    DQuestion,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    LtEq,
    GtEq,
    Eq,
    Ellipsis,

    /// A single `target: raw text` line inside an `ext` block, e.g. `py:
    /// requests.get(url).text`. Produced only while the lexer is scanning the
    /// body of an `ext ... do ... end` block (see `Lexer::in_ext_body`) — the
    /// raw text is never run through normal CuNi tokenization, since it's
    /// foreign target-language code and may contain characters CuNi's own
    /// grammar doesn't accept (`!`, `;`, `&`, ...).
    ExtTarget(String, String),

    Eof,
}

/// A piece of a backtick-interpolated string: literal text, or a nested
/// `${...}` expression captured as its own token stream (re-parsed by the
/// parser when it builds the AST).
#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Text(String),
    Expr(Vec<Tok>),
}

/// A token plus its byte span in the source file. The span is needed so the
/// parser can recover raw source text for `ext` block target lines, which
/// aren't meant to be lexed as CuNi syntax at all.
#[derive(Debug, Clone, PartialEq)]
pub struct Tok {
    pub kind: Token,
    pub start: usize,
    pub end: usize,
}

pub fn keyword(word: &str) -> Option<Token> {
    use Token::*;
    Some(match word {
        "do" => Do,
        "end" => End,
        "let" => Let,
        "mut" => Mut,
        "def" => Def,
        "link" => Link,
        "ret" => Ret,
        "fail" => Fail,
        "enum" => Enum,
        "if" => If,
        "els" => Els,
        "for" => For,
        "in" => In,
        "whl" => Whl,
        "typ" => Typ,
        "iface" => Iface,
        "is" => Is,
        "use" => Use,
        "ext" => Ext,
        "and" => And,
        "or" => Or,
        "not" => Not,
        "true" => True,
        "false" => False,
        "none" => NoneLit,
        _ => return None,
    })
}
