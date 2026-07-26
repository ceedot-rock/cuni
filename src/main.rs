mod ast;
mod check;
mod checks;
mod codegen_go;
mod codegen_js;
mod codegen_py;
mod lexer;
mod modules;
mod parser;
mod token;
mod typeck;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

fn print_usage() {
    eprintln!(
        "\
cuni — CuNi (Code:uNiTY) compiler

Usage:
  cuni check <file.cuni|dir> [--verbose] [--timeout <secs>] [--keep]
  cuni <file.cuni> [--emit-py <out.py>] [--emit-go <out.go>] [--emit-js <out.js>]
  cuni --help
  cuni --version

Commands:
  check   Exactness gate (SPEC §2): emit py/go/js, run each, require
          identical stdout. Exit 0 only on PASS.
          Prints:  exactness: PASS (py/go/js)

Emit mode:
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

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("cuni {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    if args[0] == "check" {
        return cmd_check(&args[1..]);
    }

    cmd_compile(&args)
}

fn cmd_check(args: &[String]) -> ExitCode {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut verbose = false;
    let mut keep = false;
    let mut timeout_secs: u64 = 60;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--verbose" | "-v" => {
                verbose = true;
                i += 1;
            }
            "--keep" => {
                keep = true;
                i += 1;
            }
            "--timeout" => {
                let v = args.get(i + 1).unwrap_or_else(|| {
                    eprintln!("cuni check: --timeout requires seconds");
                    std::process::exit(1);
                });
                timeout_secs = v.parse().unwrap_or_else(|_| {
                    eprintln!("cuni check: invalid --timeout value `{}`", v);
                    std::process::exit(1);
                });
                i += 2;
            }
            s if s.starts_with('-') => {
                eprintln!("cuni check: unknown flag `{}` (try --help)", s);
                return ExitCode::FAILURE;
            }
            s => {
                paths.push(PathBuf::from(s));
                i += 1;
            }
        }
    }

    if paths.is_empty() {
        eprintln!("cuni check: missing path (file.cuni or directory)");
        print_usage();
        return ExitCode::FAILURE;
    }

    let mut sources = Vec::new();
    for path in &paths {
        match check::collect_sources(path) {
            Ok(mut s) => sources.append(&mut s),
            Err(e) => {
                eprintln!("cuni check: {}", e);
                return ExitCode::FAILURE;
            }
        }
    }
    sources.sort();
    sources.dedup();

    let work_root = env::temp_dir().join(format!("cuni_check_{}", std::process::id()));
    if let Err(e) = fs::create_dir_all(&work_root) {
        eprintln!("cuni check: couldn't create temp dir: {}", e);
        return ExitCode::FAILURE;
    }

    let timeout = Duration::from_secs(timeout_secs);
    let mut failed = 0usize;
    let mut passed = 0usize;

    for src in &sources {
        let work = work_root.join(
            src.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("prog"),
        );
        let _ = fs::create_dir_all(&work);
        let report = check::check_file(src, &work, timeout);
        check::print_report(&report, verbose);
        if report.passed() {
            passed += 1;
        } else {
            failed += 1;
        }
        println!();
    }

    if sources.len() > 1 {
        println!(
            "exactness summary: {} passed, {} failed ({} files)",
            passed,
            failed,
            sources.len()
        );
    }

    if !keep {
        let _ = fs::remove_dir_all(&work_root);
    } else {
        eprintln!("cuni check: kept artifacts under {}", work_root.display());
    }

    if failed == 0 {
        if sources.len() == 1 {
            // already printed per-file PASS
        } else {
            println!("exactness: PASS (all {} files)", sources.len());
        }
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn cmd_compile(args: &[String]) -> ExitCode {
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

    let program = match check::load_program(std::path::Path::new(&path)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cuni: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Preserve detailed emit refuse messages for py/js collisions (same as before)
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
