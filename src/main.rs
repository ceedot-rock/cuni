mod ast;
mod audit;
mod bank;
mod check;
mod checks;
mod codegen_c;
mod codegen_core;
mod codegen_cadence;
mod codegen_cairo;
mod codegen_clarity;
mod codegen_go;
mod codegen_ink;
mod codegen_java;
mod codegen_js;
mod codegen_lua;
mod codegen_move;
mod codegen_php;
mod codegen_pl;
mod codegen_pas;
mod codegen_f90;
mod codegen_lisp;
mod codegen_ml;
mod codegen_r;
mod codegen_py;
mod codegen_rb;
mod codegen_rs;
mod codegen_sol;
mod codegen_solana;
mod codegen_sql;
mod codegen_vyper;
mod codegen_v;
mod codegen_d;
mod codegen_cr;
mod codegen_nim;
mod codegen_zig;

mod codegen_ada;
mod codegen_asm;
mod codegen_awk;
mod codegen_clj;
mod codegen_cob;
mod codegen_cs;
mod codegen_dart;
mod codegen_erl;
mod codegen_ex;
mod codegen_fs;
mod codegen_groovy;
mod codegen_hs;
mod codegen_hx;
mod codegen_jl;
mod codegen_kt;
mod codegen_m_objc;
mod codegen_pro;
mod codegen_ps1;
mod codegen_scala;
mod codegen_sh;
mod codegen_tcl;
mod emit;
mod ingest;
mod interp;
mod langs;
mod lexer;
mod modules;
mod oddity;
mod parser;
mod said;
mod stdlib_use;
mod token;
mod typeck;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

/// Exit-code contract. The gate is machine-readable: a shell, Rider, or
/// Studio must be able to tell "the program diverged" from "you forgot a
/// file" without parsing prose.
///   0 — exactness pass / success
///   1 — refusal or divergence (check FAIL, emit refused, run failed,
///       prove/audit FAIL)
///   2 — usage error (unknown flag, missing/invalid args) or missing
///       toolchain
/// Default `cuni check` seat set: the money seats (audit's gold set) plus
/// JavaScript — the seats a normal install can actually run. The full
/// 53-entry catalog stays available via `cuni check --all`; `--only`
/// overrides both.
const DEFAULT_CHECK_SEATS: &[&str] = &["py", "rs", "go", "java", "sql", "js"];

fn ex_usage() -> ExitCode {
    ExitCode::from(2)
}

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
cuni — CuNi (Code:uNiTY) compiler. 53 catalog entries (top 50 + 3 onchain profiles). Exactness or refuse.

Usage (the frozen surface):
  cuni check <file.cuni|dir> [--all] [--only id,id] [--json] [--receipt]
                             [--sign <keyfile>]
                             [--verbose] [--timeout <secs>] [--keep]
  cuni emit <file.cuni> [emit flags...]   (same as the bare-file form below)
  cuni run <file.cuni> [--lang py] [--timeout <secs>]
  cuni version
  cuni <file.cuni> [--emit-py <out.py>] [--emit-go <out.go>] [--emit-js <out.js>]
               [--emit <seat> <out>] [--emit-all <dir>] [--emit-top50 <dir>] [--list-langs]
  cuni --help
  cuni --version

Commands:
  check   Exactness gate: emit+run the default money seats
          (py, rs, go, java, sql, js) and require identical stdout.
          --all runs the full 53-entry catalog; --only id,id overrides both.
          Verdict counts native seats only: lowering seats (m, vb, swift,
          hack, st) may run under --all for information but never flip
          PASS/FAIL.
          Prints:  exactness: PASS (N native seats)
          --json prints ONLY the machine-readable receipt (source hash,
          cuni version, seat list, pass/fail, stdout hash) — no human output.
          --receipt also writes <file>.receipt.json.
          --sign <keyfile> adds Ed25519 signature fields to the receipt
          (keypair from `cuni audit --gen-key`); combines with --json.  emit    The compiler: emit catalog seats for a .cuni file. Same flags as
          the bare-file form. Refuses instead of emitting a wrong seat.
  run     Evaluate in-process (no emit). Optional `--lang py|go|js|…` emits a seat.
          Not a substitute for check.
  version Print `cuni <version>` (same as --version).

Extra commands (explicit; unchanged behavior):
  ingest  Reverse CuNi: CuNi-shaped subsets of py, go, js/ts, c/cpp, rs, awk,
          pl, sh, sql, wat → .cuni, or refuse. Other catalog seats: only
          artifacts carrying the CuNi lowering header (via the Python
          subset); anything else refuses. The result must pass the CuNi
          front-end or ingest refuses.
  bank    Paste N, get X. Ingest → emit → prove, or refuse. v1 --from py|cuni.
  prove   Run a foreign implementation; it must match CuNi gold stdout.
  audit   Financial Division: prove a foreign implementation against a
          money law (.cuni) and file a signed JSON receipt. The gold gate
          runs the money seats (py, rs, go, java, sql) via check; the impl
          runs by extension (.py/.go/.rs/.java/.sql/.js) and must print
          byte-identical stdout. PASS or REFUSE is always filed.
          `audit --gen-key [name]` mints an Ed25519 receipt-signing keypair.

Exit codes:
  0  exactness pass / success
  1  refusal or divergence (check FAIL, emit refused, run failed, prove/audit FAIL)
  2  usage error (unknown flag, missing/invalid args) or missing toolchain
  A divergence never shares an exit code with a usage error.

Emit flags (bare-file form and `cuni emit`):
  --emit-all DIR writes one artifact per catalog language.
  --emit SEAT OUT emits one seat; repeat the flag for any subset.
  --emit-top50 DIR emits the first 50 catalog languages into DIR
               (top 50 = quality native seats first, LANGS order).
"
    );
}

fn cmd_version() -> ExitCode {
    println!("cuni {}", env!("CARGO_PKG_VERSION"));
    ExitCode::SUCCESS
}

/// `cuni emit` — the bare-file compiler promoted to a real subcommand.
/// Identical flags and behavior to `cuni <file.cuni> [flags]`; the bare
/// form keeps working exactly as today for backward compatibility.
fn cmd_emit(args: &[String]) -> ExitCode {
    cmd_compile(args)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return if args.is_empty() {
            ex_usage()
        } else {
            ExitCode::SUCCESS
        };
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        return cmd_version();
    }

    if args[0] == "check" {
        return cmd_check(&args[1..]);
    }
    if args[0] == "emit" {
        return cmd_emit(&args[1..]);
    }
    if args[0] == "run" {
        return cmd_run(&args[1..]);
    }
    if args[0] == "version" {
        return cmd_version();
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
    if args[0] == "audit" {
        return cmd_audit(&args[1..]);
    }

    cmd_compile(&args)
}

fn cmd_check(args: &[String]) -> ExitCode {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut verbose = false;
    let mut keep = false;
    let mut timeout_secs: u64 = 60;
    let mut only: Option<Vec<String>> = None;
    let mut all = false;
    let mut receipt = false;
    let mut json = false;
    let mut sign_key: Option<String> = None;
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
            "--json" => {
                json = true;
                i += 1;
            }
            "--sign" => {
                sign_key = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                    eprintln!("cuni check: --sign requires a keyfile (see `cuni audit --gen-key`)");
                    std::process::exit(2);
                }));
                i += 2;
            }
            "--all" => {
                all = true;
                i += 1;
            }
            "--only" => {
                let v = args.get(i + 1).unwrap_or_else(|| {
                    eprintln!("cuni check: --only requires id,id");
                    std::process::exit(2);
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
                    std::process::exit(2);
                });
                timeout_secs = v.parse().unwrap_or_else(|_| {
                    eprintln!("cuni check: invalid --timeout value `{}`", v);
                    std::process::exit(2);
                });
                i += 2;
            }
            s if s.starts_with('-') => {
                eprintln!("cuni check: unknown flag `{}` (try --help)", s);
                return ex_usage();
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
        return ex_usage();
    }

    // Seat selection: --only wins; otherwise --all runs the full catalog;
    // the default is the money-seat set (py, rs, go, java, sql, js) — seats
    // a normal install can actually run.
    let only: Option<Vec<String>> = match only {
        Some(ids) => Some(ids),
        None if all => None,
        None => Some(DEFAULT_CHECK_SEATS.iter().map(|s| s.to_string()).collect()),
    };

    let mut sources = Vec::new();
    for path in &paths {
        match check::collect_sources(path) {
            Ok(mut s) => sources.append(&mut s),
            Err(e) => {
                eprintln!("cuni check: {}", e);
                return ex_usage();
            }
        }
    }
    sources.sort();
    sources.dedup();

    let work_root = work_dir("cuni_check");
    if let Err(e) = fs::create_dir_all(&work_root) {
        eprintln!("cuni check: couldn't create temp dir: {}", e);
        return ex_usage();
    }

    // Optional receipt signing (explicit new flag): reuse the audit Ed25519
    // machinery so a stranger can verify the receipt.
    let signing = match sign_key {
        Some(p) => match audit::load_signing_key(Path::new(&p)) {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("cuni check: {e}");
                return ex_usage();
            }
        },
        None => None,
    };

    let timeout = Duration::from_secs(timeout_secs);
    let mut failed = 0usize;
    let mut passed = 0usize;
    let mut missing_toolchains: Vec<String> = Vec::new();

    for src in &sources {
        let work = work_root.join(src.file_stem().and_then(|s| s.to_str()).unwrap_or("prog"));
        let _ = fs::create_dir_all(&work);
        let report = check::check_file_only(src, &work, timeout, only.as_deref());
        for seat in &report.toolchain_missing {
            if !missing_toolchains.iter().any(|s| s == seat) {
                missing_toolchains.push(seat.clone());
            }
        }
        // The receipt is the machine contract; --sign adds the Ed25519
        // signature fields so third parties can verify it.
        let rec = match &signing {
            Some(sk) => check::sign_receipt_json(&report, sk),
            None => check::receipt_json(&report),
        };
        if json {
            // Machine contract: ONLY the receipt on stdout.
            println!("{rec}");
        } else {
            check::print_report(&report, verbose);
        }
        if receipt {
            let rec_path = src.with_extension("receipt.json");
            match fs::write(&rec_path, &rec) {
                Ok(()) => eprintln!("cuni: wrote {}", rec_path.display()),
                Err(e) => eprintln!("cuni: receipt {}: {}", rec_path.display(), e),
            }
        }
        if report.passed() {
            passed += 1;
        } else {
            failed += 1;
        }
        if !json {
            println!();
        }
    }

    if !json && sources.len() > 1 {
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

    // A missing toolchain is an environment problem (exit 2), never an
    // exactness divergence (exit 1).
    if !missing_toolchains.is_empty() {
        eprintln!(
            "cuni check: toolchain missing: {} — install the seat toolchain or narrow with --only",
            missing_toolchains.join(", ")
        );
        return ex_usage();
    }

    if failed == 0 {
        if !json && sources.len() > 1 {
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
                    std::process::exit(2);
                }));
                i += 2;
            }
            "--timeout" => {
                let v = args.get(i + 1).unwrap_or_else(|| {
                    eprintln!("cuni run: --timeout requires seconds");
                    std::process::exit(2);
                });
                timeout_secs = v.parse().unwrap_or_else(|_| {
                    eprintln!("cuni run: invalid --timeout value `{}`", v);
                    std::process::exit(2);
                });
                i += 2;
            }
            s if s.starts_with('-') => {
                eprintln!("cuni run: unknown flag `{s}`");
                return ex_usage();
            }
            _ => {
                path = Some(args[i].clone());
                i += 1;
            }
        }
    }
    let Some(path) = path else {
        eprintln!("cuni run: missing file.cuni");
        return ex_usage();
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

/// Shared `--emit-X` handler for the onchain profiles (0.8.0): derive the
/// module name from the input file stem (`escrow` -> `cuni_escrow`), run the
/// profile's `generate_program`, write the artifact. Returns `Some(code)` on
/// failure, `None` on success.
fn emit_profile_artifact(
    out_path: &str,
    input_path: &str,
    program: &ast::Program,
    label: &str,
    gen: impl FnOnce(&ast::Program, &str) -> Result<String, String>,
) -> Option<ExitCode> {
    let stem = std::path::Path::new(input_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program");
    let snake: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let snake = snake.trim_matches('_').to_string();
    let mod_name = if snake.is_empty() {
        "cuni_program".to_string()
    } else {
        format!("cuni_{}", snake)
    };
    match gen(program, &mod_name) {
        Ok(source) => {
            if let Err(e) = fs::write(out_path, source) {
                eprintln!("cuni: couldn't write {}: {}", out_path, e);
                return Some(ex_usage());
            }
            eprintln!("cuni: wrote {}", out_path);
            None
        }
        Err(e) => {
            eprintln!("cuni: {} refused: {}", label, e);
            Some(ExitCode::FAILURE)
        }
    }
}

/// Shared `--emit-X-ref` handler: write the profile's standalone runnable
/// logic-core reference (`generate_reference`). Returns `Some(code)` on
/// failure, `None` on success.
fn emit_profile_reference(
    out_path: &str,
    program: &ast::Program,
    label: &str,
    gen: impl FnOnce(&ast::Program) -> Result<String, String>,
) -> Option<ExitCode> {
    match gen(program) {
        Ok(source) => {
            if let Err(e) = fs::write(out_path, source) {
                eprintln!("cuni: couldn't write {}: {}", out_path, e);
                return Some(ex_usage());
            }
            eprintln!("cuni: wrote {}", out_path);
            None
        }
        Err(e) => {
            eprintln!("cuni: {} reference refused: {}", label, e);
            Some(ExitCode::FAILURE)
        }
    }
}

fn cmd_compile(args: &[String]) -> ExitCode {
    let mut path = None;
    let mut emit_py: Option<String> = None;
    let mut emit_rb: Option<String> = None;
    let mut emit_lua: Option<String> = None;
    let mut emit_sol: Option<String> = None;
    let mut emit_solana: Option<String> = None;
    let mut emit_ink: Option<String> = None;
    let mut emit_ink_ref: Option<String> = None;
    let mut emit_move_: Option<String> = None;
    let mut emit_move_ref: Option<String> = None;
    let mut emit_vyper: Option<String> = None;
    let mut emit_vyper_ref: Option<String> = None;
    let mut emit_cairo: Option<String> = None;
    let mut emit_cairo_ref: Option<String> = None;
    let mut emit_clarity: Option<String> = None;
    let mut emit_clarity_ref: Option<String> = None;
    let mut emit_cadence: Option<String> = None;
    let mut emit_cadence_ref: Option<String> = None;
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
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-top50" {
            emit_top50 = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-top50 requires a directory");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-py" {
            emit_py = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-py requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-rb" {
            emit_rb = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-rb requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-lua" {
            emit_lua = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-lua requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-sol" {
            emit_sol = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-sol requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-solana" {
            emit_solana = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-solana requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-ink" {
            emit_ink = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-ink requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-ink-ref" {
            emit_ink_ref = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-ink-ref requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-move" {
            emit_move_ = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-move requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-move-ref" {
            emit_move_ref = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-move-ref requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-vyper" {
            emit_vyper = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-vyper requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-vyper-ref" {
            emit_vyper_ref = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-vyper-ref requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-cairo" {
            emit_cairo = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-cairo requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-cairo-ref" {
            emit_cairo_ref = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-cairo-ref requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-clarity" {
            emit_clarity = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-clarity requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-clarity-ref" {
            emit_clarity_ref = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-clarity-ref requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-cadence" {
            emit_cadence = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-cadence requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-cadence-ref" {
            emit_cadence_ref = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-cadence-ref requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-go" {
            emit_go = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-go requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit-js" {
            emit_js = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit-js requires an output path");
                std::process::exit(2);
            }));
            i += 2;
        } else if args[i] == "--emit" {
            let seat = args.get(i + 1).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit requires a seat id and an output path");
                std::process::exit(2);
            });
            let out = args.get(i + 2).cloned().unwrap_or_else(|| {
                eprintln!("cuni: --emit requires a seat id and an output path");
                std::process::exit(2);
            });
            emit_targets.push((seat, out));
            i += 3;
        } else if args[i].starts_with('-') {
            eprintln!("cuni: unknown flag `{}` (try --help)", args[i]);
            return ex_usage();
        } else {
            path = Some(args[i].clone());
            i += 1;
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            print_usage();
            return ex_usage();
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
            return ex_usage();
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    if let Some(out_path) = emit_rb {
        let rb_source = codegen_rb::generate(&program);
        if let Err(e) = fs::write(&out_path, rb_source) {
            eprintln!("cuni: couldn't write {}: {}", out_path, e);
            return ex_usage();
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    if let Some(out_path) = emit_lua {
        match codegen_lua::generate(&program) {
            Ok(lua_source) => {
                if let Err(e) = fs::write(&out_path, lua_source) {
                    eprintln!("cuni: couldn't write {}: {}", out_path, e);
                    return ex_usage();
                }
                eprintln!("cuni: wrote {}", out_path);
                emitted_any = true;
            }
            Err(e) => {
                eprintln!("cuni: Lua refused: {}", e);
                return ExitCode::FAILURE;
            }
        }
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
                    return ex_usage();
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
    if let Some(out_path) = emit_solana {
        // Program module from the input file stem: escrow -> cuni_escrow.
        let stem = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("program");
        let snake: String = stem
            .chars()
            .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        let snake = snake.trim_matches('_').to_string();
        let prog_mod = if snake.is_empty() {
            "cuni_program".to_string()
        } else {
            format!("cuni_{}", snake)
        };
        match codegen_solana::generate_program(&program, &prog_mod) {
            Ok(program_source) => {
                if let Err(e) = fs::write(&out_path, program_source) {
                    eprintln!("cuni: couldn't write {}: {}", out_path, e);
                    return ex_usage();
                }
                eprintln!("cuni: wrote {}", out_path);
                emitted_any = true;
            }
            Err(e) => {
                eprintln!("cuni: Solana refused: {}", e);
                return ExitCode::FAILURE;
            }
        }
    }
    // Onchain Division profiles (0.8.0). Each is a --emit-X / --emit-X-ref
    // pair: the chain artifact plus the standalone runnable logic reference
    // the gate proves byte-identical to CuNi gold.
    if let Some(out_path) = emit_ink {
        if let Some(code) =
            emit_profile_artifact(&out_path, &path, &program, "ink!", codegen_ink::generate_program)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_ink_ref {
        if let Some(code) =
            emit_profile_reference(&out_path, &program, "ink!", codegen_ink::generate_reference)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_move_ {
        if let Some(code) =
            emit_profile_artifact(&out_path, &path, &program, "Move", codegen_move::generate_program)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_move_ref {
        if let Some(code) =
            emit_profile_reference(&out_path, &program, "Move", codegen_move::generate_reference)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_vyper {
        if let Some(code) =
            emit_profile_artifact(&out_path, &path, &program, "Vyper", codegen_vyper::generate_program)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_vyper_ref {
        if let Some(code) =
            emit_profile_reference(&out_path, &program, "Vyper", codegen_vyper::generate_reference)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_cairo {
        if let Some(code) =
            emit_profile_artifact(&out_path, &path, &program, "Cairo", codegen_cairo::generate_program)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_cairo_ref {
        if let Some(code) =
            emit_profile_reference(&out_path, &program, "Cairo", codegen_cairo::generate_reference)
        {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_clarity {
        if let Some(code) = emit_profile_artifact(
            &out_path,
            &path,
            &program,
            "Clarity",
            codegen_clarity::generate_program,
        ) {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_clarity_ref {
        if let Some(code) = emit_profile_reference(
            &out_path,
            &program,
            "Clarity",
            codegen_clarity::generate_reference,
        ) {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_cadence {
        if let Some(code) = emit_profile_artifact(
            &out_path,
            &path,
            &program,
            "Cadence",
            codegen_cadence::generate_program,
        ) {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_cadence_ref {
        if let Some(code) = emit_profile_reference(
            &out_path,
            &program,
            "Cadence",
            codegen_cadence::generate_reference,
        ) {
            return code;
        }
        emitted_any = true;
    }
    if let Some(out_path) = emit_go {
        match codegen_go::generate(&program) {
            Ok(go_source) => {
                if let Err(e) = fs::write(&out_path, go_source) {
                    eprintln!("cuni: couldn't write {}: {}", out_path, e);
                    return ex_usage();
                }
                eprintln!("cuni: wrote {}", out_path);
                emitted_any = true;
            }
            Err(e) => {
                eprintln!("cuni: Go refused: {}", e);
                return ExitCode::FAILURE;
            }
        }
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
            return ex_usage();
        }
        eprintln!("cuni: wrote {}", out_path);
        emitted_any = true;
    }
    for (seat_id, out_path) in &emit_targets {
        let lang = match langs::LANGS.iter().find(|l| l.id == seat_id.as_str()) {
            Some(l) => l,
            None => {
                eprintln!("cuni: unknown seat `{}` (try --list-langs)", seat_id);
                return ex_usage();
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
            return ex_usage();
        }
        eprintln!("cuni: wrote {} ({})", out_path, seat_id);
        emitted_any = true;
    }
    if let Some(dir) = emit_all {
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("cuni: couldn't create {}: {}", dir, e);
            return ex_usage();
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
                return ex_usage();
            }
        }
        eprintln!("cuni: wrote {} languages to {}", langs::LANGS.len(), dir);
        emitted_any = true;
    }
    if let Some(dir) = emit_top50 {
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("cuni: couldn't create {}: {}", dir, e);
            return ex_usage();
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
            return ex_usage();
        } else {
            input = Some(args[i].clone());
            i += 1;
        }
    }
    let Some(input) = input else {
        eprintln!("cuni ingest: missing file.py");
        return ex_usage();
    };
    match ingest::ingest_file(Path::new(&input)) {
        Ok(cuni) => {
            if let Some(out) = output {
                if let Err(e) = fs::write(&out, &cuni) {
                    eprintln!("cuni ingest: {e}");
                    return ex_usage();
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

/// The gold gate, shared by `prove` and `audit`: run `check` on the CuNi
/// source restricted to `only` seats, and return the seats that ran plus the
/// py seat's stdout as gold. Reuses `check::check_file_only` — no duplicate
/// machinery.
fn run_gold_gate(cuni_path: &Path, only: &[&str]) -> Result<audit::GoldGate, audit::GoldGateError> {
    let work = work_dir("cuni_gold");
    let _ = fs::create_dir_all(&work);
    let only: Vec<String> = only.iter().map(|s| s.to_string()).collect();
    let report = check::check_file_only(cuni_path, &work, Duration::from_secs(180), Some(&only));
    let seats_run: Vec<String> = report
        .targets
        .iter()
        .filter(|t| t.run_ok)
        .map(|t| t.target.to_string())
        .collect();
    if !report.passed() {
        return Err(audit::GoldGateError {
            reason: format!("CuNi gold failed exactness\n{}", report.summary),
            seats_run,
        });
    }
    let gold = check::gold_stdout(&report)
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default();
    Ok(audit::GoldGate {
        seats_run,
        gold_stdout: gold,
    })
}

fn cmd_audit(args: &[String]) -> ExitCode {
    // Key generation mode: `cuni audit --gen-key [name]`.
    if args.first().map(|s| s.as_str()) == Some("--gen-key") {
        let name = args.get(1).cloned().unwrap_or_else(|| "auditor".to_string());
        if name.starts_with('-') {
            eprintln!("cuni audit: --gen-key takes an optional key name, got `{name}`");
            return ex_usage();
        }
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        match audit::gen_keypair(&name, &cwd) {
            Ok((key_path, pub_path)) => {
                println!("audit: keypair written");
                println!("  secret: {} (mode 600 — guard it)", key_path.display());
                println!("  public: {}", pub_path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("cuni audit: keygen refused: {e}");
                ExitCode::FAILURE
            }
        }
    } else {
        cmd_audit_law(args)
    }
}

fn cmd_audit_law(args: &[String]) -> ExitCode {
    let mut law_path = None;
    let mut against = None;
    let mut signer = None;
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--against" {
            against = args.get(i + 1).cloned();
            i += 2;
        } else if args[i] == "--signer" {
            signer = args.get(i + 1).cloned();
            i += 2;
        } else if args[i] == "--out" {
            out = args.get(i + 1).cloned();
            i += 2;
        } else if args[i].starts_with('-') {
            eprintln!("cuni audit: unknown flag `{}`", args[i]);
            return ex_usage();
        } else if law_path.is_none() {
            law_path = Some(args[i].clone());
            i += 1;
        } else {
            eprintln!("cuni audit: unexpected argument `{}`", args[i]);
            return ex_usage();
        }
    }
    let Some(law_path) = law_path else {
        eprintln!("cuni audit: missing law.cuni (or use `cuni audit --gen-key [name]`)");
        return ex_usage();
    };
    let Some(against) = against else {
        eprintln!("cuni audit: --against <impl> required");
        return ex_usage();
    };

    let signing = match signer {
        Some(keyfile) => match audit::load_signing_key(Path::new(&keyfile)) {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("cuni audit: {e}");
                return ex_usage();
            }
        },
        None => None,
    };

    // The gold gate runs the money seats (audit::GOLD_SEATS); py is in the
    // list because the gold stdout is defined as the py seat's stdout.
    let receipt = match audit::audit_law(
        Path::new(&law_path),
        Path::new(&against),
        signing.as_ref(),
        |p| run_gold_gate(p, audit::GOLD_SEATS),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cuni audit: {e}");
            return ExitCode::FAILURE;
        }
    };
    let json = serde_json::to_string(&receipt).unwrap_or_else(|_| "{}".to_string());
    if let Some(out) = out {
        if let Err(e) = fs::write(&out, format!("{json}\n")) {
            eprintln!("cuni audit: write {out}: {e}");
            return ex_usage();
        }
        eprintln!("audit: {} — receipt filed at {out}", receipt.verdict);
    } else {
        println!("{json}");
    }
    if receipt.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
            return ex_usage();
        } else {
            cuni_path = Some(args[i].clone());
            i += 1;
        }
    }
    let Some(cuni_path) = cuni_path else {
        eprintln!("cuni prove: missing file.cuni");
        return ex_usage();
    };
    let Some(against) = against else {
        eprintln!("cuni prove: --against <impl> required");
        return ex_usage();
    };
    // Same gold machinery as audit (prove keeps its lighter py/go/js seat
    // set; audit uses the money seats).
    let gate = match run_gold_gate(Path::new(&cuni_path), &["py", "go", "js"]) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("cuni prove: {}", e.reason);
            return ExitCode::FAILURE;
        }
    };
    let against_path = PathBuf::from(&against);
    // Same foreign-impl runner as audit (py/go/js/rs/java/sql).
    match audit::run_foreign_impl(&against_path, Duration::from_secs(120)) {
        Ok(got) => {
            if got == gate.gold_stdout {
                println!("prove: PASS — {} matches CuNi gold", against);
                ExitCode::SUCCESS
            } else {
                let gold = String::from_utf8_lossy(&gate.gold_stdout);
                let got_s = String::from_utf8_lossy(&got);
                eprintln!("prove: FAIL — {} diverged from CuNi gold", against);
                eprintln!("  --- gold ---\n{gold}  --- impl ---\n{got_s}");
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("prove: FAIL — {e}");
            ExitCode::FAILURE
        }
    }
}
