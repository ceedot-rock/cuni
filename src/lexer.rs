use crate::token::{keyword, StrPart, Tok, Token};

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    /// True once the lexer has scanned an `ext` keyword and is still looking
    /// for the `do` that starts its body (there's no other `do` possible in
    /// an `ext` header, so this is unambiguous).
    after_ext_header: bool,
    /// True while scanning inside `ext ... do <here> end` — target lines are
    /// read as raw text, never tokenized as CuNi (see `ExtTarget`).
    in_ext_body: bool,
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub pos: usize,
}

type LexResult<T> = Result<T, LexError>;

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer { src: src.as_bytes(), pos: 0, after_ext_header: false, in_ext_body: false }
    }

    pub fn tokenize(src: &'a str) -> LexResult<Vec<Tok>> {
        let mut lexer = Lexer::new(src);
        lexer.run()
    }

    fn run(&mut self) -> LexResult<Vec<Tok>> {
        let mut out = Vec::new();
        loop {
            if self.in_ext_body {
                self.skip_ascii_whitespace();
                let start = self.pos;
                if self.pos >= self.src.len() {
                    return Err(self.err("unterminated ext block"));
                }
                let kind = self.scan_ext_body_token()?;
                out.push(Tok { kind, start, end: self.pos });
                continue;
            }

            self.skip_trivia();
            let start = self.pos;
            if self.pos >= self.src.len() {
                out.push(Tok { kind: Token::Eof, start, end: start });
                break;
            }
            let kind = self.next_token()?;
            if kind == Token::Ext {
                self.after_ext_header = true;
            }
            if kind == Token::Do && self.after_ext_header {
                self.after_ext_header = false;
                self.in_ext_body = true;
            }
            out.push(Tok { kind, start, end: self.pos });
        }
        Ok(out)
    }

    fn skip_ascii_whitespace(&mut self) {
        while self.peek().map_or(false, |c| c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    /// Reads one `target: raw text` line, or `end` to close the block. The
    /// raw text runs to end-of-line and is never interpreted as CuNi syntax.
    fn scan_ext_body_token(&mut self) -> LexResult<Token> {
        if self.src[self.pos..].starts_with(b"end") {
            let after = self.pos + 3;
            let word_continues = self.src.get(after).map_or(false, |c| c.is_ascii_alphanumeric() || *c == b'_');
            if !word_continues {
                self.pos = after;
                self.in_ext_body = false;
                return Ok(Token::End);
            }
        }

        let name_start = self.pos;
        while self.peek().map_or(false, |c| c == b'_' || c.is_ascii_alphanumeric()) {
            self.pos += 1;
        }
        if self.pos == name_start {
            return Err(self.err("expected a target name (e.g. 'py') or 'end' in ext block"));
        }
        let name = std::str::from_utf8(&self.src[name_start..self.pos]).unwrap().to_string();

        while self.peek().map_or(false, |c| c == b' ' || c == b'\t') {
            self.pos += 1;
        }
        if self.peek() != Some(b':') {
            return Err(self.err("expected ':' after target name in ext block"));
        }
        self.pos += 1;
        while self.peek().map_or(false, |c| c == b' ' || c == b'\t') {
            self.pos += 1;
        }

        let text_start = self.pos;
        while self.peek().map_or(false, |c| c != b'\n') {
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.src[text_start..self.pos]).unwrap().trim_end().to_string();
        Ok(Token::ExtTarget(name, raw))
    }

    fn skip_trivia(&mut self) {
        loop {
            while self.peek().map_or(false, |c| c.is_ascii_whitespace()) {
                self.pos += 1;
            }
            if self.peek() == Some(b'#') {
                while self.peek().map_or(false, |c| c != b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    /// Reads one full UTF-8 character (not just one byte) and advances past
    /// it. `self.src` came from a `&str`, and `self.pos` is always at a char
    /// boundary as long as every advance elsewhere is either an ASCII byte
    /// (1 byte == 1 char) or a full multi-byte char via this method — so the
    /// slice from `pos` onward is always valid UTF-8.
    fn bump_char(&mut self) -> char {
        let rest = std::str::from_utf8(&self.src[self.pos..]).expect("source is valid utf8 at a char boundary");
        let ch = rest.chars().next().expect("bump_char called at end of input");
        self.pos += ch.len_utf8();
        ch
    }

    fn next_token(&mut self) -> LexResult<Token> {
        let c = self.bump().unwrap();
        match c {
            b'(' => Ok(Token::LParen),
            b')' => Ok(Token::RParen),
            b'{' => Ok(Token::LBrace),
            b'}' => Ok(Token::RBrace),
            b'[' => Ok(Token::LBracket),
            b']' => Ok(Token::RBracket),
            b',' => Ok(Token::Comma),
            b':' => Ok(Token::Colon),
            b'+' => Ok(Token::Plus),
            b'*' => Ok(Token::Star),
            b'/' => Ok(Token::Slash),
            b'%' => Ok(Token::Percent),
            b'.' => {
                if self.peek() == Some(b'.') && self.peek_at(1) == Some(b'.') {
                    self.pos += 2;
                    Ok(Token::Ellipsis)
                } else {
                    Ok(Token::Dot)
                }
            }
            b'-' => {
                if self.peek() == Some(b'>') {
                    self.pos += 1;
                    Ok(Token::Arrow)
                } else {
                    Ok(Token::Minus)
                }
            }
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Ok(Token::EqEq)
                } else {
                    Ok(Token::Eq)
                }
            }
            b'!' => {
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Ok(Token::NotEq)
                } else {
                    Err(self.err("'!' must be followed by '=' — CuNi has no unary '!', use 'not'"))
                }
            }
            b'<' => {
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Ok(Token::LtEq)
                } else {
                    Ok(Token::Lt)
                }
            }
            b'>' => {
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Ok(Token::GtEq)
                } else {
                    Ok(Token::Gt)
                }
            }
            b'?' => {
                if self.peek() == Some(b'?') {
                    self.pos += 1;
                    Ok(Token::DQuestion)
                } else {
                    Ok(Token::Question)
                }
            }
            b'"' => self.lex_plain_string(),
            b'`' => self.lex_interp_string().map(Token::InterpStr),
            b'0'..=b'9' => self.lex_number(),
            c if c == b'_' || c.is_ascii_alphabetic() => Ok(self.lex_ident()),
            other => Err(self.err(&format!("unexpected character '{}'", other as char))),
        }
    }

    fn err(&self, message: &str) -> LexError {
        LexError { message: message.to_string(), pos: self.pos }
    }

    fn lex_ident(&mut self) -> Token {
        let start = self.pos - 1;
        while self.peek().map_or(false, |c| c == b'_' || c.is_ascii_alphanumeric()) {
            self.pos += 1;
        }
        let word = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        keyword(word).unwrap_or_else(|| Token::Ident(word.to_string()))
    }

    fn lex_number(&mut self) -> LexResult<Token> {
        let start = self.pos - 1;
        while self.peek().map_or(false, |c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
            is_float = true;
            self.pos += 1;
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        if is_float {
            text.parse::<f64>()
                .map(Token::Float)
                .map_err(|_| self.err("invalid float literal"))
        } else {
            text.parse::<i64>()
                .map(Token::Int)
                .map_err(|_| self.err("invalid int literal"))
        }
    }

    /// Plain `"..."` string — no interpolation, so it's read verbatim
    /// (with the usual backslash escapes) into a single Str token.
    fn lex_plain_string(&mut self) -> LexResult<Token> {
        let mut s = String::new();
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 1;
                    s.push(self.read_escape()?);
                }
                Some(_) => s.push(self.bump_char()),
                None => return Err(self.err("unterminated string literal")),
            }
        }
        Ok(Token::Str(s))
    }

    /// Skips over a `"..."` string body (escapes included) without
    /// interpreting it — used while scanning past a `${...}` expression so a
    /// `}` inside a nested string literal doesn't end the interpolation early.
    fn skip_over_plain_string_body(&mut self) -> LexResult<()> {
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(b'\\') => self.pos += 2,
                Some(_) => self.pos += 1,
                None => return Err(self.err("unterminated string literal inside ${...}")),
            }
        }
    }

    /// Same idea as `skip_over_plain_string_body` but for a backtick string —
    /// doesn't attempt to track further nested `${...}` inside it (a rare
    /// edge case beyond this toy lexer's scope), just avoids treating its
    /// `}`/`{` as interpolation braces.
    fn skip_over_interp_string_body(&mut self) -> LexResult<()> {
        loop {
            match self.peek() {
                Some(b'`') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(b'\\') => self.pos += 2,
                Some(_) => self.pos += 1,
                None => return Err(self.err("unterminated interpolated string inside ${...}")),
            }
        }
    }

    /// Backtick string with `${expr}` interpolation. Each `${...}` segment is
    /// lexed recursively (by re-entering the tokenizer on that slice) so the
    /// parser can later parse it as a normal expression.
    fn lex_interp_string(&mut self) -> LexResult<Vec<StrPart>> {
        let mut parts = Vec::new();
        let mut text = String::new();
        loop {
            match self.peek() {
                Some(b'`') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 1;
                    text.push(self.read_escape()?);
                }
                Some(b'$') if self.peek_at(1) == Some(b'{') => {
                    if !text.is_empty() {
                        parts.push(StrPart::Text(std::mem::take(&mut text)));
                    }
                    self.pos += 2; // consume ${
                    let expr_start = self.pos;
                    let mut depth = 1;
                    while depth > 0 {
                        match self.peek() {
                            Some(b'{') => {
                                depth += 1;
                                self.pos += 1;
                            }
                            Some(b'}') => {
                                depth -= 1;
                                self.pos += 1;
                            }
                            Some(b'"') => {
                                self.pos += 1;
                                self.skip_over_plain_string_body()?;
                            }
                            Some(b'`') => {
                                self.pos += 1;
                                self.skip_over_interp_string_body()?;
                            }
                            Some(_) => self.pos += 1,
                            None => return Err(self.err("unterminated ${...} in interpolated string")),
                        }
                    }
                    let inner_src = std::str::from_utf8(&self.src[expr_start..self.pos - 1]).unwrap();
                    let mut inner_toks = Lexer::tokenize(inner_src)?;
                    inner_toks.retain(|t| t.kind != Token::Eof);
                    parts.push(StrPart::Expr(inner_toks));
                }
                Some(_) => {
                    text.push(self.bump_char());
                }
                None => return Err(self.err("unterminated interpolated string")),
            }
        }
        if !text.is_empty() {
            parts.push(StrPart::Text(text));
        }
        Ok(parts)
    }

    fn read_escape(&mut self) -> LexResult<char> {
        match self.bump() {
            Some(b'n') => Ok('\n'),
            Some(b't') => Ok('\t'),
            Some(b'"') => Ok('"'),
            Some(b'`') => Ok('`'),
            Some(b'\\') => Ok('\\'),
            Some(b'$') => Ok('$'),
            _ => Err(self.err("invalid escape sequence")),
        }
    }
}
