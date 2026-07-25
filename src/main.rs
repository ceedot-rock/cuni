mod ast;
mod checks;
mod codegen_go;
mod codegen_js;
mod codegen_py;
mod lexer;
mod modules;
mod parser;
mod token;
mod typeck;

use lexer::Lexer;
use parser::Parser;
use std::env;
use std::fs;
use std::process::ExitCode;

fn line_col(source: &str, byte_pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_pos {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn print_usage() {
    eprintln!(
        "\
cuni — CuNi (Code:uNiTY) compiler

Usage:
  cuni <file.cuni> [--emit-py <out.py>] [--emit-go <out.go>] [--emit-js <out.js>]
  cuni --help

With no --emit-* flags, prints the parsed AST (debug) after type-checking.
`use name` loads <dir>/<name>.cuni relative to the source file.
"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return if args.is_empty() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    let mut path = None;
    let mut emit_py: Option<String> = None;
    let mut emit_go: Option<String> = None;
    let mut emit_js: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--emit-py" {
            emit_py = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-py requires an output path");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i] == "--emit-go" {
            emit_go = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-go requires an output path");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i] == "--emit-js" {
            emit_js = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-js requires an output path");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i].starts_with('-') {
            eprintln!("cuni: unknown flag `{}` (try --help)", args[i]);
            return ExitCode::FAILURE;
        } else {
            path = Some(args[i].clone());
            i += 1;
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cuni: couldn't read {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };

    let tokens = match Lexer::tokenize(&source) {
        Ok(t) => t,
        Err(e) => {
            let (line, col) = line_col(&source, e.pos);
            eprintln!("{}:{}:{}: lex error: {}", path, line, col, e.message);
            return ExitCode::FAILURE;
        }
    };

    let mut parser = Parser::new(tokens, &source);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            let (line, col) = line_col(&source, e.pos);
            eprintln!("{}:{}:{}: parse error: {}", path, line, col, e.message);
            return ExitCode::FAILURE;
        }
    };

    let program = match modules::resolve_uses(program, std::path::Path::new(&path)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}: module error: {}", path, e.message);
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = typeck::check_program(&program) {
        eprintln!("{}: type error: {}", path, e.message);
        return ExitCode::FAILURE;
    }
    if let Some((name, reason)) = checks::find_bad_link_type(&program) {
        eprintln!(
            "cuni: refusing to compile: `link {}` has a non-scalar {} — link v1 only supports int/float/str/bool (see SPEC.md §16)",
            name, reason
        );
        return ExitCode::FAILURE;
    }

    let mut emitted_any = false;
    if let Some(out_path) = emit_py {
        if let Some(name) = checks::find_ext_collision(&program, "py") {
            eprintln!(
                "cuni: refusing to compile for py: `ext {}` shadows the Python builtin `{}` inside its own py: body — rename the CuNi binding (see OPEN_ITEMS_PROPOSAL.md item 5)",
                name, name
            );
            return ExitCode::FAILURE;
        }
        let py_source = codegen_py::generate(&program);
        if let Err(e) = fs::write(&out_path, py_source) {
            eprintln!("cuni: couldn't write {}: {}", out_path, e);
            return ExitCode::FAILURE;
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    if let Some(out_path) = emit_go {
        let go_source = codegen_go::generate(&program);
        if let Err(e) = fs::write(&out_path, go_source) {
            eprintln!("cuni: couldn't write {}: {}", out_path, e);
            return ExitCode::FAILURE;
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    if let Some(out_path) = emit_js {
        if let Some(name) = checks::find_ext_collision(&program, "js") {
            eprintln!(
                "cuni: refusing to compile for js: `ext {}` shadows the JS global `{}` inside its own js: body — rename the CuNi binding (see OPEN_ITEMS_PROPOSAL.md item 5)",
                name, name
            );
            return ExitCode::FAILURE;
        }
        let js_source = codegen_js::generate(&program);
        if let Err(e) = fs::write(&out_path, js_source) {
            eprintln!("cuni: couldn't write {}: {}", out_path, e);
            return ExitCode::FAILURE;
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    if !emitted_any {
        println!("{:#?}", program);
    }
    ExitCode::SUCCESS
}
