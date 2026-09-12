//! CuNi library: parse + in-process run. The CLI bin stays `src/main.rs`.

mod ast;
mod lexer;
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
}
