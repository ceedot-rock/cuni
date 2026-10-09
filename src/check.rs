//! Exactness check: emit every catalog language, run each, require identical
//! stdout. SPEC.md §2 — compile-or-refuse.
//!
//! Exit-code contract (enforced by the CLI in main.rs, not here):
//! 0 = PASS, 1 = FAIL (refusal or divergence), 2 = usage error or missing
//! toolchain. A missing seat binary is reported via
//! [`CheckReport::toolchain_missing`] so the caller can return 2 — a
//! divergence must never share an exit code with "you forgot a file".

use crate::ast::{Item, Program};
use crate::checks;
use crate::emit;
use crate::interp;
use crate::langs::{self, Lang};
use crate::lexer::Lexer;
use crate::modules;
use crate::parser::Parser;
use crate::said;
use crate::typeck;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;


#[derive(Debug, Clone)]
pub struct TargetResult {
    pub target: &'static str,
    pub emit_ok: bool,
    pub emit_err: Option<String>,
    pub run_ok: bool,
    pub run_err: Option<String>,
    pub stdout: Option<String>,
    pub compares_stdout: bool,
}

#[derive(Debug)]
pub struct CheckReport {
    pub path: PathBuf,
    pub source_hash: String,
    pub front_ok: bool,
    pub front_err: Option<String>,
    pub targets: Vec<TargetResult>,
    /// Seat ids whose toolchain binary was not found on this machine.
    /// A missing toolchain is an environment problem (CLI exit 2), never
    /// an exactness divergence (exit 1).
    pub toolchain_missing: Vec<String>,
    pub exact: bool,
    pub summary: String,
}

impl CheckReport {
    pub fn passed(&self) -> bool {
        self.exact
    }
}

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

/// Load, parse, resolve modules, type-check, and static refuse checks.
pub fn load_program(path: &Path) -> Result<Program, String> {
    let source =
        fs::read_to_string(path).map_err(|e| format!("couldn't read {}: {}", path.display(), e))?;

    let tokens = Lexer::tokenize(&source).map_err(|e| {
        let (line, col) = line_col(&source, e.pos);
        format!(
            "{}:{}:{}: lex error: {}",
            path.display(),
            line,
            col,
            e.message
        )
    })?;

    let mut parser = Parser::new(tokens, &source);
    let program = parser.parse_program().map_err(|e| {
        let (line, col) = line_col(&source, e.pos);
        format!(
            "{}:{}:{}: parse error: {}",
            path.display(),
            line,
            col,
            e.message
        )
    })?;

    let program = modules::resolve_uses(program, path).map_err(|e| {
        if let Some(span) = e.span {
            let (line, col) = line_col(&source, span.start);
            format!(
                "{}:{}:{}: module error: {}",
                path.display(),
                line,
                col,
                e.message
            )
        } else {
            format!("{}: module error: {}", path.display(), e.message)
        }
    })?;

    typeck::check_program(&program).map_err(|e| {
        let (line, col) = line_col(&source, e.span.start);
        format!(
            "{}:{}:{}: type error: {}",
            path.display(),
            line,
            col,
            e.message
        )
    })?;

    if let Some((name, reason)) = checks::find_bad_link_type(&program) {
        return Err(format!(
            "refusing to compile: `link {}` has a non-scalar {} — link v1 only supports int/float/str/bool",
            name, reason
        ));
    }

    Ok(program)
}

fn emit_for(program: &Program, lang: &Lang, out: &Path) -> Result<(), String> {
    if lang.id == "py" || emit::seat_kind(lang) == emit::SeatKind::Lowering {
        if let Some(name) = checks::find_ext_collision(program, "py") {
            return Err(format!(
                "`ext {}` shadows the Python builtin `{}` inside its own py: body",
                name, name
            ));
        }
    }
    if lang.id == "js" || lang.id == "ts" {
        if let Some(name) = checks::find_ext_collision(program, "js") {
            return Err(format!(
                "`ext {}` shadows the JS global `{}` inside its own js: body",
                name, name
            ));
        }
    }
    fs::write(out, emit::generate_exact(program, lang)?).map_err(|e| e.to_string())
}

/// A failed seat run. `toolchain_missing` is true when the seat's binary
/// could not be spawned at all (ErrorKind::NotFound) — the CLI maps that
/// to exit 2, distinct from a genuine run failure / divergence (exit 1).
#[derive(Debug, Clone)]
pub struct RunFail {
    pub message: String,
    pub toolchain_missing: bool,
}

fn run_target(lang: &Lang, artifact: &Path, timeout: Duration) -> Result<String, RunFail> {
    let plan = emit::exec_plan(lang, artifact);
    if let Some((cmd, args)) = plan.compile {
        run_blocking(&cmd, &args, timeout)?;
    }
    run_blocking(&plan.run.0, &plan.run.1, timeout)
}

fn run_blocking(cmd: &str, args: &[String], timeout: Duration) -> Result<String, RunFail> {
    use std::sync::mpsc;
    let cmd_name = cmd.to_string();
    let cmd = cmd_name.clone();
    let args = args.to_vec();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let output = Command::new(&cmd).args(&args).output();
        let _ = tx.send(output);
    });
    let fail = |message: String, toolchain_missing: bool| RunFail {
        message,
        toolchain_missing,
    };
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(fail(
                    format!(
                        "exit {}\nstderr: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    false,
                ))
            }
        }
        Ok(Err(e)) => {
            let missing = e.kind() == std::io::ErrorKind::NotFound;
            Err(fail(
                if missing {
                    format!(
                        "toolchain missing: `{}` not found on PATH ({})",
                        cmd_name, e
                    )
                } else {
                    format!("failed to run: {}", e)
                },
                missing,
            ))
        }
        Err(_) => Err(fail(format!("timeout after {}s", timeout.as_secs()), false)),
    }
}

/// Check one `.cuni` source for cross-target exactness.
#[allow(dead_code)]
pub fn check_file(path: &Path, work_dir: &Path, timeout: Duration) -> CheckReport {
    check_file_only(path, work_dir, timeout, None)
}

/// A seat counts toward the exactness verdict only when its catalog mark
/// is `native`. Lowering seats (m, vb, swift, hack, st) may run under
/// `--all` for information, but neither their passes nor their failures
/// affect PASS/FAIL.
fn is_native_seat(id: &str) -> bool {
    matches!(
        langs::LANGS
            .iter()
            .find(|l| l.id == id)
            .map(emit::seat_kind),
        Some(emit::SeatKind::Native) | None
    )
}

/// Native-only exactness verdict over already-run targets.
/// Returns (exact, summary).
fn exactness_verdict(targets: &[TargetResult], program: &Program) -> (bool, String) {
    let native: Vec<&TargetResult> =
        targets.iter().filter(|t| is_native_seat(t.target)).collect();
    if native.is_empty() {
        return (
            false,
            "exactness: FAIL — no native seats ran; the gate counts native seats only (lowering seats m, vb, swift, hack, st are informational)\nfix-it: run without --only, or pick native seat ids".into(),
        );
    }
    let n = native.len();
    let all_ok = native.iter().all(|t| t.emit_ok && t.run_ok);
    if !all_ok {
        let mut parts = Vec::new();
        for t in &native {
            if let Some(e) = &t.emit_err {
                parts.push(format!("{} emit refused: {}", t.target, e));
            } else if let Some(e) = &t.run_err {
                parts.push(format!(
                    "{} run failed: {}",
                    t.target,
                    e.lines().next().unwrap_or("")
                ));
            }
        }
        return (
            false,
            format!(
                "exactness: FAIL — {}\nfix-it: every native seat must emit+run; fix the first failing target, then re-run `cuni check` (no approximate mode)",
                parts.join("; ")
            ),
        );
    }

    let gold = native
        .iter()
        .find(|t| t.compares_stdout)
        .and_then(|t| t.stdout.as_deref())
        .unwrap_or("");
    let mut diverged: Vec<&str> = Vec::new();
    let mut compile_only: Vec<&str> = Vec::new();
    for t in &native {
        if !t.compares_stdout {
            // Compile-verified seats (e.g. sol: solc proves deployability)
            // don't produce program stdout; their values are cross-checked
            // against the interpreter instead.
            if t.emit_ok && t.run_ok {
                compile_only.push(t.target);
            }
            continue;
        }
        if t.stdout.as_deref().unwrap_or("") != gold {
            diverged.push(t.target);
        }
    }
    if !diverged.is_empty() {
        let show: Vec<_> = diverged.iter().take(8).copied().collect();
        return (
            false,
            format!(
                "exactness: FAIL — stdout diverged vs {} for: {}{}\nfix-it: remove `ext` host differences, avoid non-portable float printing, keep integer/`say` paths identical — CuNi has no approximate mode",
                native[0].target,
                show.join(", "),
                if diverged.len() > 8 {
                    format!(" (+{} more)", diverged.len() - 8)
                } else {
                    String::new()
                }
            ),
        );
    }

    let has_ext = program.items.iter().any(|i| matches!(i, Item::Ext(_)));
    let has_stdout_seat = native.iter().any(|t| t.compares_stdout);
    if !has_ext && has_stdout_seat {
        match interp::run(program) {
            Ok(ref got) if got == gold => {}
            Ok(_) => {
                return (
                    false,
                    format!(
                        "exactness: FAIL — interp stdout diverged vs {}\nfix-it: the in-process runner is a seat; it must print the same as py/go/js (no approximate mode)",
                        native[0].target
                    ),
                );
            }
            Err(e) => {
                return (
                    false,
                    format!(
                        "exactness: FAIL — interp: {}\nfix-it: portable programs must run in-process; `ext` is the only skip",
                        e.lines().next().unwrap_or("run failed")
                    ),
                );
            }
        }
    }

    let summary = if compile_only.is_empty() {
        format!("exactness: PASS ({} native seats)", n)
    } else {
        format!(
            "exactness: PASS ({} native seats; compile-only, no stdout: {})",
            n,
            compile_only.join(", ")
        )
    };
    (true, summary)
}

pub fn check_file_only(
    path: &Path,
    work_dir: &Path,
    timeout: Duration,
    only: Option<&[String]>,
) -> CheckReport {
    let source = fs::read_to_string(path).unwrap_or_default();
    let mut report = CheckReport {
        path: path.to_path_buf(),
        source_hash: said::said(&source),
        front_ok: false,
        front_err: None,
        targets: Vec::new(),
        toolchain_missing: Vec::new(),
        exact: false,
        summary: String::new(),
    };

    let program = match load_program(path) {
        Ok(p) => {
            report.front_ok = true;
            p
        }
        Err(e) => {
            report.front_err = Some(e.clone());
            report.summary = format!(
                "exactness: FAIL — front-end: {}\nfix-it: resolve the type/parse error above; exactness never runs on a refused program (no approximate mode)",
                e
            );
            return report;
        }
    };

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("prog");

    let langs: Vec<&Lang> = langs::LANGS
        .iter()
        .filter(|l| {
            only.map(|ids| ids.iter().any(|id| id == l.id || id == l.ext))
                .unwrap_or(true)
        })
        .collect();
    if langs.is_empty() {
        report.summary = "exactness: FAIL — --only matched no catalog languages".into();
        return report;
    }

    for lang in langs {
        let out = work_dir.join(format!("{}_{}", stem, lang.out_file()));
        let mut tr = TargetResult {
            target: lang.id,
            emit_ok: false,
            emit_err: None,
            run_ok: false,
            run_err: None,
            stdout: None,
            compares_stdout: true,
        };
        // Seats that don't produce program stdout (e.g. sol: a contract has
        // no EVM here) are emit+compile verified; their computed values are
        // cross-checked against the interpreter instead.
        tr.compares_stdout = emit::exec_plan(lang, &out).compares_stdout;
        match emit_for(&program, lang, &out) {
            Ok(()) => {
                tr.emit_ok = true;
                match run_target(lang, &out, timeout) {
                    Ok(stdout) => {
                        tr.run_ok = true;
                        tr.stdout = Some(stdout);
                    }
                    Err(f) => {
                        if f.toolchain_missing
                            && !report.toolchain_missing.iter().any(|s| s == lang.id)
                        {
                            report.toolchain_missing.push(lang.id.to_string());
                        }
                        tr.run_err = Some(f.message);
                    }
                }
            }
            Err(e) => tr.emit_err = Some(e),
        }
        report.targets.push(tr);
    }

    let (exact, summary) = exactness_verdict(&report.targets, &program);
    report.exact = exact;
    report.summary = summary;
    report
}

/// Collect `.cuni` files: single file, or recursive directory walk.
pub fn collect_sources(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("cuni") {
            return Ok(vec![path.to_path_buf()]);
        }
        return Err(format!("{} is not a .cuni file", path.display()));
    }
    if path.is_dir() {
        let mut files = Vec::new();
        collect_dir(path, &mut files);
        files.sort();
        if files.is_empty() {
            return Err(format!("no .cuni files under {}", path.display()));
        }
        return Ok(files);
    }
    Err(format!("path not found: {}", path.display()))
}

fn collect_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            // skip target/ and .git
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            collect_dir(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("cuni") {
            out.push(p);
        }
    }
}

pub fn print_report(report: &CheckReport, verbose: bool) {
    let label = report.path.display();
    println!("check {}", label);
    if !report.front_ok {
        println!(
            "  front-end  FAIL  {}",
            report.front_err.as_deref().unwrap_or("")
        );
        println!("  {}", report.summary);
        return;
    }
    println!("  front-end  ok");
    let n = report.targets.len();
    let ok_n = report
        .targets
        .iter()
        .filter(|t| t.emit_ok && t.run_ok)
        .count();
    if report.exact && !verbose {
        println!("  emit/run {}/{} ok", ok_n, n);
    } else {
        for t in &report.targets {
            if !t.emit_ok {
                println!(
                    "  emit {:<8}  REFUSE  {}",
                    t.target,
                    t.emit_err.as_deref().unwrap_or("")
                );
                continue;
            }
            if verbose {
                println!("  emit {:<8}  ok", t.target);
            }
            if !t.run_ok {
                println!(
                    "  run  {:<8}  FAIL  {}",
                    t.target,
                    t.run_err
                        .as_deref()
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                );
            } else if verbose {
                println!("  run  {:<8}  ok", t.target);
                if let Some(s) = &t.stdout {
                    for line in s.lines() {
                        println!("           | {}", line);
                    }
                    if s.is_empty() {
                        println!("           | (empty stdout)");
                    }
                }
            }
        }
        if !verbose {
            println!("  emit/run {}/{} ok", ok_n, n);
        }
    }
    if report.exact {
        println!("  {}", report.summary);
    } else {
        // multi-line fail detail
        for (i, line) in report.summary.lines().enumerate() {
            if i == 0 {
                println!("  {}", line);
            } else {
                println!("  {}", line);
            }
        }
    }
}

/// Machine-readable exactness receipt. This is the contract the future
/// `POST /v1/check` returns: it runs `cuni check --json` and hands back
/// this object. Fields are additive-only — never rename or remove one.
///   path, source_hash, exact, summary, langs — stable since 0.9.0
///   seats — per-seat {id, seat (native|lowering), emit, run}
///   seats_ran — seat ids whose run step actually executed
///   stdout_hash — sha256 hex of the gold stdout ("" when there is no gold)
///   cuni_version — pinned compiler version; a pass from one version is
///     never read as a pass from another
///   signature / signer_pubkey — present only when the receipt was made
///     with `cuni check --sign <keyfile>`. Scheme: signature =
///     hex(Ed25519_sign(secret_key, unsigned_bytes)) where unsigned_bytes
///     is the exact UTF-8 of `receipt_json()` (no signature fields).
///     Verify: strip the two signature fields, re-render the unsigned
///     form byte-identically, and verify against signer_pubkey.
pub fn receipt_json(report: &CheckReport) -> String {
    receipt_json_inner(report, None)
}

/// Signed receipt: `receipt_json()` bytes signed with the audit Ed25519
/// machinery (`cuni audit --gen-key` mints the keypair; the keyfile holds
/// the 32 raw secret bytes `load_signing_key` expects).
pub fn sign_receipt_json(report: &CheckReport, signing: &SigningKey) -> String {
    let unsigned = receipt_json(report);
    let sig = signing.sign(unsigned.as_bytes());
    receipt_json_inner(
        report,
        Some((&hex_of(&signing.verifying_key().to_bytes()), &hex_of(&sig.to_bytes()))),
    )
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn receipt_json_inner(report: &CheckReport, sig: Option<(&str, &str)>) -> String {
    let mut seats = String::from("[");
    let mut seats_ran = String::from("[");
    let mut first_ran = true;
    for (i, t) in report.targets.iter().enumerate() {
        if i > 0 {
            seats.push(',');
        }
        let kind = langs::LANGS
            .iter()
            .find(|l| l.id == t.target)
            .map(emit::seat_kind)
            .unwrap_or(emit::SeatKind::Lowering);
        let kind = match kind {
            emit::SeatKind::Native => "native",
            emit::SeatKind::Lowering => "lowering",
        };
        seats.push_str(&format!(
            "{{\"id\":\"{}\",\"seat\":\"{}\",\"emit\":{},\"run\":{}}}",
            t.target, kind, t.emit_ok, t.run_ok
        ));
        if t.run_ok {
            if !first_ran {
                seats_ran.push(',');
            }
            first_ran = false;
            seats_ran.push_str(&format!("{:?}", t.target));
        }
    }
    seats.push(']');
    seats_ran.push(']');
    let gold = gold_stdout_for_receipt(report);
    let stdout_hash = if gold.is_empty() {
        String::new()
    } else {
        hex_sha256(gold.as_bytes())
    };
    let sig_fields = match sig {
        Some((pubkey, signature)) => format!(
            ",\n  \"signer_pubkey\": {:?},\n  \"signature\": {:?}",
            pubkey, signature
        ),
        None => String::new(),
    };
    format!(
        "{{\n  \"path\": {:?},\n  \"source_hash\": {:?},\n  \"cuni_version\": {:?},\n  \"exact\": {},\n  \"summary\": {:?},\n  \"langs\": {},\n  \"seats_ran\": {},\n  \"stdout_hash\": {:?},\n  \"seats\": {}{}\n}}\n",
        report.path.display().to_string(),
        report.source_hash,
        env!("CARGO_PKG_VERSION"),
        report.exact,
        report.summary,
        report.targets.len(),
        seats_ran,
        stdout_hash,
        seats,
        sig_fields
    )
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Gold stdout for the receipt: first stdout-comparing seat's output.
/// Empty string when no seat produced stdout (e.g. front-end refusal).
fn gold_stdout_for_receipt(report: &CheckReport) -> &str {
    report
        .targets
        .iter()
        .find(|t| t.compares_stdout)
        .and_then(|t| t.stdout.as_deref())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_report() -> CheckReport {
        CheckReport {
            path: PathBuf::from("ignored.cuni"),
            source_hash: String::new(),
            front_ok: true,
            front_err: None,
            targets: vec![],
            toolchain_missing: Vec::new(),
            exact: true,
            summary: "exactness: PASS (0 langs)".into(),
        }
    }

    #[test]
    fn receipt_names_the_program_not_the_path() {
        let src = "fn main() { say(1) }\n";
        let mut report = empty_report();
        report.source_hash = said::said(src);
        let rec = receipt_json(&report);
        assert!(rec.contains("source_hash"));
        assert!(rec.contains(&said::said(src)));
        assert_ne!(said::said(src), said::said("fn main() { say(2) }\n"));
    }

    #[test]
    fn receipt_carries_the_full_contract() {
        let src = "fn main() { say(1) }\n";
        let mut report = empty_report();
        report.source_hash = said::said(src);
        report.targets = vec![
            TargetResult {
                target: "py",
                emit_ok: true,
                emit_err: None,
                run_ok: true,
                run_err: None,
                stdout: Some("1\n".into()),
                compares_stdout: true,
            },
            TargetResult {
                target: "rs",
                emit_ok: true,
                emit_err: None,
                run_ok: true,
                run_err: None,
                stdout: Some("1\n".into()),
                compares_stdout: true,
            },
        ];
        let rec = receipt_json(&report);
        let v: serde_json::Value =
            serde_json::from_str(&rec).expect("receipt must parse as JSON");
        // contract fields
        for field in [
            "path",
            "source_hash",
            "cuni_version",
            "exact",
            "summary",
            "langs",
            "seats_ran",
            "stdout_hash",
            "seats",
        ] {
            assert!(v.get(field).is_some(), "receipt missing `{}`", field);
        }
        assert_eq!(v["cuni_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(v["exact"], true);
        assert_eq!(v["langs"], 2);
        // stdout_hash is sha256 hex of the gold stdout ("1\n")
        let mut h = Sha256::new();
        h.update(b"1\n");
        assert_eq!(v["stdout_hash"], format!("{:x}", h.finalize()));
        // per-seat entries carry id + emit/run status
        assert_eq!(v["seats"][0]["id"], "py");
        assert_eq!(v["seats"][0]["emit"], true);
        assert_eq!(v["seats"][0]["run"], true);
        assert_eq!(v["seats"][1]["id"], "rs");
        // seats_ran lists the seats that actually ran
        let ran: Vec<&str> = v["seats_ran"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        assert_eq!(ran, vec!["py", "rs"]);
    }

    #[test]
    fn receipt_empty_stdout_hash_when_no_gold() {
        let report = empty_report();
        let rec = receipt_json(&report);
        let v: serde_json::Value = serde_json::from_str(&rec).unwrap();
        assert_eq!(v["stdout_hash"], "");
        assert!(v["seats_ran"].as_array().unwrap().is_empty());
    }

    fn target(id: &'static str, emit_ok: bool, run_ok: bool) -> TargetResult {
        TargetResult {
            target: id,
            emit_ok,
            emit_err: None,
            run_ok,
            run_err: None,
            stdout: Some("".into()),
            compares_stdout: true,
        }
    }

    fn empty_program() -> Program {
        Program { items: vec![] }
    }

    #[test]
    fn verdict_ignores_lowering_seat_failure() {
        // swift is a lowering seat; its failure must not flip the verdict
        // when every native seat passes.
        let targets = vec![
            target("py", true, true),
            target("rs", true, true),
            target("go", true, true),
            target("swift", true, false),
        ];
        let (exact, summary) = exactness_verdict(&targets, &empty_program());
        assert!(exact, "lowering-seat failure must not fail the gate: {summary}");
        assert!(summary.contains("PASS"));
    }

    #[test]
    fn verdict_fails_when_native_seat_fails() {
        let targets = vec![target("py", true, true), target("rs", true, false)];
        let (exact, _) = exactness_verdict(&targets, &empty_program());
        assert!(!exact);
    }

    #[test]
    fn verdict_fails_with_no_native_seats() {
        let targets = vec![target("swift", true, true), target("m", true, true)];
        let (exact, summary) = exactness_verdict(&targets, &empty_program());
        assert!(!exact);
        assert!(summary.contains("no native seats"));
    }

    #[test]
    fn signed_receipt_verifies_against_unsigned_bytes() {
        use ed25519_dalek::{Signature, Verifier};
        let mut report = empty_report();
        report.targets = vec![target("py", true, true)];
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let signed = sign_receipt_json(&report, &sk);
        let v: serde_json::Value = serde_json::from_str(&signed).unwrap();
        assert!(v.get("signature").is_some());
        assert!(v.get("signer_pubkey").is_some());
        // unsigned form has no signature fields
        let unsigned = receipt_json(&report);
        assert!(!unsigned.contains("signature"));
        // signature verifies over the exact unsigned bytes
        let sig_hex = v["signature"].as_str().unwrap();
        let sig_bytes: Vec<u8> = (0..sig_hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&sig_hex[i..i + 2], 16).unwrap())
            .collect();
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&sig_bytes);
        sk.verifying_key()
            .verify(unsigned.as_bytes(), &Signature::from_bytes(&arr))
            .expect("signature must verify");
    }
}

/// Gold stdout: Python native seat of a passing (or attempted) check.
pub fn gold_stdout(report: &CheckReport) -> Option<&str> {
    report
        .targets
        .iter()
        .find(|t| t.target == "py" && t.run_ok)
        .and_then(|t| t.stdout.as_deref())
}

const RUN_SEATS: &[&str] = &["py", "go", "js", "ts", "c", "cpp", "rs"];

/// Emit and run one native seat. Not a substitute for `cuni check`.
pub fn run_one(
    path: &Path,
    lang_id: &str,
    work_dir: &Path,
    timeout: Duration,
) -> Result<String, String> {
    let lang = langs::LANGS
        .iter()
        .find(|l| l.id == lang_id || l.ext == lang_id)
        .ok_or_else(|| format!("unknown language `{lang_id}`"))?;
    if !RUN_SEATS.contains(&lang.id) {
        return Err(format!(
            "`{lang_id}` is not a native seat — use py,go,js,ts,c,cpp,rs (`cuni run` is one seat; `cuni check` is the proof)"
        ));
    }
    let program = load_program(path)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("prog");
    let _ = fs::create_dir_all(work_dir);
    let out = work_dir.join(format!("{}_{}", stem, lang.out_file()));
    emit_for(&program, lang, &out)?;
    run_target(lang, &out, timeout).map_err(|f| f.message)
}
