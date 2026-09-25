//! CuNi library: parse + in-process run for Studio/CLI.
//! Not a compressor front-end. Exactness remains `cuni check`.

mod ast;
mod ir;
mod lexer;
mod oddity;
mod parser;
mod token;
mod typeck;
mod interp;

pub use interp::run;

/// Parse and evaluate CuNi source in-process. No emit, no `use` modules.
pub fn run_source(src: &str) -> Result<String, String> {
    let tokens = lexer::Lexer::tokenize(src).map_err(|e| e.message)?;
    let mut parser = parser::Parser::new(tokens, src);
    let program = parser.parse_program().map_err(|e| e.message)?;
    if program
        .items
        .iter()
        .any(|i| matches!(i, ast::Item::Use(_)))
    {
        return Err("run_source refuses `use` (needs files)".into());
    }
    typeck::check_program(&program).map_err(|e| e.message)?;
    interp::run(&program)
}

#[cfg(test)]
mod tests {
    use super::ast::{Item, Span};
    use super::lexer::Lexer;
    use super::parser::Parser;

    #[test]
    fn run_source_fib() {
        let src = r#"
def fib(n: int) -> int do
    if n < 2 do
        ret n
    end
    ret fib(n - 1) + fib(n - 2)
end
say(fib(10))
"#;
        assert_eq!(super::run_source(src).unwrap(), "55\n");
    }

    fn parse(src: &str) -> super::ast::Program {
        let tokens = Lexer::tokenize(src).expect("lex");
        Parser::new(tokens, src).parse_program().expect("parse")
    }

    fn slice(src: &str, span: Span) -> &str {
        &src[span.start..span.end]
    }

    /// M1 span coverage: Use name, enum variant names, iface method names.
    #[test]
    fn ast_public_name_spans_cover_source_bytes() {
        let src = "\
use math\n\
\n\
enum Color do\n\
    Red\n\
    Green\n\
end\n\
\n\
iface Shape do\n\
    area() -> float\n\
end\n";
        let prog = parse(src);

        let mut saw_use = false;
        let mut saw_enum = false;
        let mut saw_iface = false;
        for item in &prog.items {
            match item {
                Item::Use(u) => {
                    saw_use = true;
                    assert_eq!(slice(src, u.name_span), "math");
                    assert!(u.name_span.end > u.name_span.start);
                }
                Item::Enum(e) => {
                    saw_enum = true;
                    assert_eq!(slice(src, e.name_span), "Color");
                    assert_eq!(e.variants.len(), 2);
                    assert_eq!(slice(src, e.variants[0].name_span), "Red");
                    assert_eq!(slice(src, e.variants[1].name_span), "Green");
                    assert_eq!(e.variants[0].name, "Red");
                    assert_eq!(e.variants[1].name, "Green");
                }
                Item::Iface(i) => {
                    saw_iface = true;
                    assert_eq!(slice(src, i.name_span), "Shape");
                    assert_eq!(i.methods.len(), 1);
                    assert_eq!(slice(src, i.methods[0].name_span), "area");
                    assert_eq!(i.methods[0].name, "area");
                }
                _ => {}
            }
        }
        assert!(saw_use && saw_enum && saw_iface);
    }

    #[test]
    fn duplicate_enum_variant_refuses_at_variant_span() {
        let src = "enum Color do\n    Red\n    Red\nend\n";
        let prog = parse(src);
        let err = super::typeck::check_program(&prog).expect_err("dup variant");
        assert!(
            err.message.contains("duplicate variant `Red`"),
            "{}",
            err.message
        );
        assert_eq!(slice(src, err.span), "Red");
        // Second `Red` — span must not be the first occurrence.
        let first = src.find("Red").unwrap();
        assert!(err.span.start > first);
    }
}
