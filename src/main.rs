mod ast;
mod bank;
mod check;
mod checks;
mod codegen_c;
mod codegen_go;
mod codegen_java;
mod codegen_js;
mod codegen_lua;
mod codegen_py;
mod codegen_rb;
mod codegen_rs;
mod codegen_sol;
mod codegen_sql;
mod emit;
mod ingest;
mod interp;
mod langs;
mod lexer;
mod modules;
mod oddity;
mod parser;
mod said;
mod token;
mod typeck;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

/// Unique-per-invocation temp dir. The old scheme (`{prefix}_{pid}`) raced
/// when one parent spawned several `cuni` processes at once (same pid for
/// every child): concurrent gates stomped each other's artifacts. Pid +
/// nanos makes collisions practically impossible.
fn work_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
}

fn print_usage() {
    eprintln!(
        "\
cuni — CuNi (Code:uNiTY) compiler. 144 languages. Exactness or refuse.

Usage:
  cuni check <file.cuni|dir> [--verbose] [--timeout <secs>] [--keep] [--only id,id] [--receipt]
  cuni run <file.cuni> [--lang py] [--timeout <secs>]
  cuni ingest <file.ext> [-o out.cuni]
  cuni bank paste <file> --from py --to <id> [-o out]
  cuni prove <file.cuni> --against <impl>
  cuni <file.cuni> [--emit-py <out.py>] [--emit-go <out.go>] [--emit-js <out.js>]
               [--emit <seat> <out>] [--emit-all <dir>] [--emit-top50 <dir>] [--list-langs]
  cuni --help
  cuni --version

Commands:
  check   Exactness gate: emit+run every catalog language (or --only).
          Native seats today: py, go, js, ts, c, cpp, rs, rb, lua, sol, java, sql.
          Other ids: Python lowering so the 144-language gate still runs.
          Prints:  exactness: PASS (N langs)
  run     Evaluate in-process (no emit). Optional `--lang py|go|js|…` emits a seat.
          Not a substitute for check.
  ingest  Reverse CuNi: CuNi-shaped subsets of py, go, js/ts, c/cpp, rs, awk,
          pl, sh, sql, wat → .cuni, or refuse. Other catalog seats: only
          artifacts carrying the CuNi lowering header (via the Python
          subset); anything else refuses. The result must pass the CuNi
          front-end or ingest refuses.
  bank    Paste N, get X. Ingest → emit → prove, or refuse. v1 --from py|cuni.
  prove   Run a foreign implementation; it must match CuNi gold stdout.

Emit:
  --emit-all DIR writes one artifact per catalog language.
  --emit SEAT OUT emits one seat; repeat the flag for any subset.
  --emit-top50 DIR emits the first 50 catalog languages into DIR
               (top 50 = quality native seats first, LANGS order).
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
    if args[0] == "run" {
        return cmd_run(&args[1..]);
    }
    if args[0] == "ingest" {
        return cmd_ingest(&args[1..]);
    }
    if args[0] == "bank" {
        return bank::cmd_bank(&args[1..]);
    }
    if args[0] == "prove" {
        return cmd_prove(&args[1..]);
    }

    cmd_compile(&args)
}

fn cmd_check(args: &[String]) -> ExitCode {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut verbose = false;
    let mut keep = false;
    let mut timeout_secs: u64 = 60;
    let mut only: Option<Vec<String>> = None;
    let mut receipt = false;
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
            "--receipt" => {
                receipt = true;
                i += 1;
            }
            "--only" => {
                let v = args.get(i + 1).unwrap_or_else(|| {
                    eprintln!("cuni check: --only requires id,id");
                    std::process::exit(1);
                });
                only = Some(
                    v.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                );
                i += 2;
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

    let work_root = work_dir("cuni_check");
    if let Err(e) = fs::create_dir_all(&work_root) {
        eprintln!("cuni check: couldn't create temp dir: {}", e);
        return ExitCode::FAILURE;
    }

    let timeout = Duration::from_secs(timeout_secs);
    let mut failed = 0usize;
    let mut passed = 0usize;

    for src in &sources {
        let work = work_root.join(src.file_stem().and_then(|s| s.to_str()).unwrap_or("prog"));
        let _ = fs::create_dir_all(&work);
        let report = check::check_file_only(src, &work, timeout, only.as_deref());
        check::print_report(&report, verbose);
        if receipt {
            let rec = check::receipt_json(&report);
            let rec_path = src.with_extension("receipt.json");
            match fs::write(&rec_path, rec) {
                Ok(()) => eprintln!("cuni: wrote {}", rec_path.display()),
                Err(e) => eprintln!("cuni: receipt {}: {}", rec_path.display(), e),
            }
        }
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

fn cmd_run(args: &[String]) -> ExitCode {
    let mut path = None;
    let mut lang: Option<String> = None;
    let mut timeout_secs: u64 = 60;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--lang" => {
                lang = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                    eprintln!("cuni run: --lang requires an id (py,go,js,ts,c,cpp,rs)");
                    std::process::exit(1);
                }));
                i += 2;
            }
            "--timeout" => {
                let v = args.get(i + 1).unwrap_or_else(|| {
                    eprintln!("cuni run: --timeout requires seconds");
                    std::process::exit(1);
                });
                timeout_secs = v.parse().unwrap_or_else(|_| {
                    eprintln!("cuni run: invalid --timeout value `{}`", v);
                    std::process::exit(1);
                });
                i += 2;
            }
            s if s.starts_with('-') => {
                eprintln!("cuni run: unknown flag `{s}`");
                return ExitCode::FAILURE;
            }
            _ => {
                path = Some(args[i].clone());
                i += 1;
            }
        }
    }
    let Some(path) = path else {
        eprintln!("cuni run: missing file.cuni");
        return ExitCode::FAILURE;
    };
    let program = match check::load_program(Path::new(&path)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cuni run: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = if let Some(lang) = lang {
        let work = work_dir("cuni_run");
        let _ = fs::create_dir_all(&work);
        check::run_one(
            Path::new(&path),
            &lang,
            &work,
            Duration::from_secs(timeout_secs),
        )
    } else {
        interp::run(&program)
    };
    match result {
        Ok(stdout) => {
            print!("{stdout}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cuni run: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Emit one catalog seat to `out_path`, or refuse with a reason.
/// Shared by `--emit`, `--emit-top50`, and (via --emit-all's own loop) kept
/// consistent with the ext-collision rule for py/js (see item 5).
fn emit_seat_to(
    program: &ast::Program,
    lang: &langs::Lang,
    out_path: &str,
) -> Result<(), String> {
    let seat_id = lang.id;
    if seat_id == "py" || seat_id == "js" {
        if let Some(name) = checks::find_ext_collision(program, seat_id) {
            let what = if seat_id == "py" {
                "Python builtin"
            } else {
                "JS global"
            };
            return Err(format!(
                "refusing to compile for {seat_id}: `ext {name}` shadows the {what} `{name}` inside its own {seat_id}: body — rename the CuNi binding (see OPEN_ITEMS_PROPOSAL.md item 5)"
            ));
        }
    }
    let src = emit::generate_exact(program, lang)
        .map_err(|e| format!("emit refused for {seat_id}: {e}"))?;
    fs::write(out_path, src).map_err(|e| format!("couldn't write {out_path}: {e}"))?;
    Ok(())
}

fn cmd_compile(args: &[String]) -> ExitCode {
    let mut path = None;
    let mut emit_py: Option<String> = None;
    let mut emit_rb: Option<String> = None;
    let mut emit_lua: Option<String> = None;
    let mut emit_sol: Option<String> = None;
    let mut emit_go: Option<String> = None;
    let mut emit_js: Option<String> = None;
    let mut emit_all: Option<String> = None;
    let mut emit_top50: Option<String> = None;
    let mut emit_targets: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--list-langs" {
            for l in langs::LANGS {
                println!("{}\t{}\t.{}", l.id, l.name, l.ext);
            }
            return ExitCode::SUCCESS;
        } else if args[i] == "--emit-all" {
            emit_all = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-all requires a directory");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i] == "--emit-top50" {
            emit_top50 = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-top50 requires a directory");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i] == "--emit-py" {
            emit_py = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-py requires an output path");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i] == "--emit-rb" {
            emit_rb = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-rb requires an output path");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i] == "--emit-lua" {
            emit_lua = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-lua requires an output path");
                std::process::exit(1);
            }));
            i += 2;
        } else if args[i] == "--emit-sol" {
            emit_sol = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-sol requires an output path");
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
        } else if args[i] == "--emit" {
            let seat = args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit requires a seat id and an output path");
                std::process::exit(1);
            });
            let out = args.get(i + 2).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit requires a seat id and an output path");
                std::process::exit(1);
            });
            emit_targets.push((seat, out));
            i += 3;
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
    if let Some(out_path) = emit_rb {
        let rb_source = codegen_rb::generate(&program);
        if let Err(e) = fs::write(&out_path, rb_source) {
            eprintln!("cuni: couldn't write {}: {}", out_path, e);
            return ExitCode::FAILURE;
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    if let Some(out_path) = emit_lua {
        let lua_source = codegen_lua::generate(&program);
        if let Err(e) = fs::write(&out_path, lua_source) {
            eprintln!("cuni: couldn't write {}: {}", out_path, e);
            return ExitCode::FAILURE;
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    if let Some(out_path) = emit_sol {
        // Contract name from the input file stem: provably-fair-dice -> ProvablyFairDice.
        let stem = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("contract");
        let contract = stem
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<String>();
        let contract = if contract.is_empty() {
            "CuniContract".to_string()
        } else {
            contract
        };
        match codegen_sol::generate_named(&program, &contract) {
            Ok(sol_source) => {
                if let Err(e) = fs::write(&out_path, sol_source) {
                    eprintln!("cuni: couldn't write {}: {}", out_path, e);
                    return ExitCode::FAILURE;
                }
                eprintln!("cuni: wrote {}", out_path);
                emitted_any = true;
            }
            Err(e) => {
                eprintln!("cuni: Solidity refused: {}", e);
                return ExitCode::FAILURE;
            }
        }
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
    for (seat_id, out_path) in &emit_targets {
        let lang = match langs::LANGS.iter().find(|l| l.id == seat_id.as_str()) {
            Some(l) => l,
            None => {
                eprintln!("cuni: unknown seat `{}` (try --list-langs)", seat_id);
                return ExitCode::FAILURE;
            }
        };
        if seat_id == "py" || seat_id == "js" {
            if let Some(name) = checks::find_ext_collision(&program, seat_id) {
                let what = if seat_id == "py" {
                    "Python builtin"
                } else {
                    "JS global"
                };
                eprintln!(
                    "cuni: refusing to compile for {}: `ext {}` shadows the {} `{}` inside its own {}: body — rename the CuNi binding (see OPEN_ITEMS_PROPOSAL.md item 5)",
                    seat_id, name, what, name, seat_id
                );
                return ExitCode::FAILURE;
            }
        }
        let src = match emit::generate_exact(&program, lang) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cuni: emit refused for {}: {}", seat_id, e);
                return ExitCode::FAILURE;
            }
        };
        if let Err(e) = fs::write(&out_path, src) {
            eprintln!("cuni: couldn't write {}: {}", out_path, e);
            return ExitCode::FAILURE;
        }
        eprintln!("cuni: wrote {} ({})", out_path, seat_id);
        emitted_any = true;
    }
    if let Some(dir) = emit_all {
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("cuni: couldn't create {}: {}", dir, e);
            return ExitCode::FAILURE;
        }
        for lang in langs::LANGS {
            let src = match emit::generate_exact(&program, lang) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("cuni: emit refused for {}: {}", lang.id, e);
                    return ExitCode::FAILURE;
                }
            };
            let path = format!("{}/{}", dir, lang.out_file());
            if let Err(e) = fs::write(&path, src) {
                eprintln!("cuni: couldn't write {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        }
        eprintln!("cuni: wrote {} languages to {}", langs::LANGS.len(), dir);
        emitted_any = true;
    }
    if let Some(dir) = emit_top50 {
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("cuni: couldn't create {}: {}", dir, e);
            return ExitCode::FAILURE;
        }
        let top: Vec<&langs::Lang> = langs::LANGS.iter().take(50).collect();
        for lang in &top {
            let path = format!("{}/{}", dir, lang.out_file());
            if let Err(e) = emit_seat_to(&program, lang, &path) {
                eprintln!("cuni: {}", e);
                return ExitCode::FAILURE;
            }
        }
        eprintln!("cuni: wrote top 50 languages to {}", dir);
        emitted_any = true;
    }
    if !emitted_any {
        println!("{:#?}", program);
    }
    ExitCode::SUCCESS
}

fn cmd_ingest(args: &[String]) -> ExitCode {
    let mut input = None;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-o" || args[i] == "--output" {
            output = args.get(i + 1).cloned();
            i += 2;
        } else if args[i].starts_with('-') {
            eprintln!("cuni ingest: unknown flag `{}`", args[i]);
            return ExitCode::FAILURE;
        } else {
            input = Some(args[i].clone());
            i += 1;
        }
    }
    let Some(input) = input else {
        eprintln!("cuni ingest: missing file.py");
        return ExitCode::FAILURE;
    };
    match ingest::ingest_file(Path::new(&input)) {
        Ok(cuni) => {
            if let Some(out) = output {
                if let Err(e) = fs::write(&out, &cuni) {
                    eprintln!("cuni ingest: {e}");
                    return ExitCode::FAILURE;
                }
                eprintln!("cuni: ingested {} → {}", input, out);
            } else {
                print!("{cuni}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cuni: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_prove(args: &[String]) -> ExitCode {
    let mut cuni_path = None;
    let mut against = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--against" {
            against = args.get(i + 1).cloned();
            i += 2;
        } else if args[i].starts_with('-') {
            eprintln!("cuni prove: unknown flag `{}`", args[i]);
            return ExitCode::FAILURE;
        } else {
            cuni_path = Some(args[i].clone());
            i += 1;
        }
    }
    let Some(cuni_path) = cuni_path else {
        eprintln!("cuni prove: missing file.cuni");
        return ExitCode::FAILURE;
    };
    let Some(against) = against else {
        eprintln!("cuni prove: --against <impl> required");
        return ExitCode::FAILURE;
    };
    let work = work_dir("cuni_prove");
    let _ = fs::create_dir_all(&work);
    let report = check::check_file_only(
        PathBuf::from(&cuni_path).as_path(),
        &work,
        Duration::from_secs(120),
        Some(&["py".to_string(), "go".to_string(), "js".to_string()]),
    );
    if !report.passed() {
        eprintln!("cuni prove: CuNi gold failed exactness\n{}", report.summary);
        return ExitCode::FAILURE;
    }
    let Some(gold) = check::gold_stdout(&report).map(|s| s.to_string()) else {
        eprintln!("cuni prove: no Python gold stdout");
        return ExitCode::FAILURE;
    };
    let against_path = PathBuf::from(&against);
    let ext = against_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    let (cmd, cmd_args): (String, Vec<String>) = match ext.as_str() {
        "py" => ("python3".into(), vec![against.clone()]),
        "js" | "mjs" => ("node".into(), vec![against.clone()]),
        "go" => ("go".into(), vec!["run".into(), against.clone()]),
        _ => {
            eprintln!("cuni prove: refuse unknown impl seat `.{}`", ext);
            return ExitCode::FAILURE;
        }
    };
    let output = std::process::Command::new(&cmd).args(&cmd_args).output();
    match output {
        Ok(o) if o.status.success() => {
            let got = String::from_utf8_lossy(&o.stdout);
            if got.as_ref() == gold {
                println!("prove: PASS — {} matches CuNi gold", against);
                ExitCode::SUCCESS
            } else {
                eprintln!("prove: FAIL — {} diverged from CuNi gold", against);
                eprintln!("  --- gold ---\n{gold}  --- impl ---\n{got}");
                ExitCode::FAILURE
            }
        }
        Ok(o) => {
            eprintln!(
                "prove: FAIL — {} exited {}\n{}",
                against,
                o.status,
                String::from_utf8_lossy(&o.stderr)
            );
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("prove: FAIL — {e}");
            ExitCode::FAILURE
        }
    }
}
