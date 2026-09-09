//! Exactness check: emit every catalog language, run each, require identical
//! stdout. SPEC.md §2 — compile-or-refuse. Exit 0 = PASS, 1 = FAIL.

use crate::ast::Program;
use crate::checks;
use crate::emit;
use crate::langs::{self, Lang};
use crate::lexer::Lexer;
use crate::modules;
use crate::parser::Parser;
use crate::said;
use crate::typeck;
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
}

#[derive(Debug)]
pub struct CheckReport {
    pub path: PathBuf,
    pub source_hash: String,
    pub front_ok: bool,
    pub front_err: Option<String>,
    pub targets: Vec<TargetResult>,
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
    let source = fs::read_to_string(path).map_err(|e| format!("couldn't read {}: {}", path.display(), e))?;

    let tokens = Lexer::tokenize(&source).map_err(|e| {
        let (line, col) = line_col(&source, e.pos);
        format!("{}:{}:{}: lex error: {}", path.display(), line, col, e.message)
    })?;

    let mut parser = Parser::new(tokens, &source);
    let program = parser.parse_program().map_err(|e| {
        let (line, col) = line_col(&source, e.pos);
        format!("{}:{}:{}: parse error: {}", path.display(), line, col, e.message)
    })?;

    let program = modules::resolve_uses(program, path).map_err(|e| {
        format!("{}: module error: {}", path.display(), e.message)
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
    fs::write(out, emit::generate_exact(program, lang)).map_err(|e| e.to_string())
}

fn run_target(lang: &Lang, artifact: &Path, timeout: Duration) -> Result<String, String> {
    let plan = emit::exec_plan(lang, artifact);
    if let Some((cmd, args)) = plan.compile {
        run_blocking(&cmd, &args, timeout)?;
    }
    run_blocking(&plan.run.0, &plan.run.1, timeout)
}

fn run_blocking(cmd: &str, args: &[String], timeout: Duration) -> Result<String, String> {
    use std::sync::mpsc;
    let cmd = cmd.to_string();
    let args = args.to_vec();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let output = Command::new(&cmd).args(&args).output();
        let _ = tx.send(output);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(format!(
                    "exit {}\nstderr: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        }
        Ok(Err(e)) => Err(format!("failed to run: {}", e)),
        Err(_) => Err(format!("timeout after {}s", timeout.as_secs())),
    }
}

/// Check one `.cuni` source for cross-target exactness.
#[allow(dead_code)]
pub fn check_file(path: &Path, work_dir: &Path, timeout: Duration) -> CheckReport {
    check_file_only(path, work_dir, timeout, None)
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
            report.summary = format!("exactness: FAIL — front-end: {}", e);
            return report;
        }
    };

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("prog");

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
        };
        match emit_for(&program, lang, &out) {
            Ok(()) => {
                tr.emit_ok = true;
                match run_target(lang, &out, timeout) {
                    Ok(stdout) => {
                        tr.run_ok = true;
                        tr.stdout = Some(stdout);
                    }
                    Err(e) => tr.run_err = Some(e),
                }
            }
            Err(e) => tr.emit_err = Some(e),
        }
        report.targets.push(tr);
    }

    let n = report.targets.len();
    let all_ok = report.targets.iter().all(|t| t.emit_ok && t.run_ok);
    if !all_ok {
        let mut parts = Vec::new();
        for t in &report.targets {
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
        report.summary = format!("exactness: FAIL — {}", parts.join("; "));
        report.exact = false;
        return report;
    }

    let gold = report.targets[0].stdout.as_deref().unwrap_or("");
    let mut diverged: Vec<&str> = Vec::new();
    for t in &report.targets {
        if t.stdout.as_deref().unwrap_or("") != gold {
            diverged.push(t.target);
        }
    }
    if diverged.is_empty() {
        report.exact = true;
        report.summary = format!("exactness: PASS ({} langs)", n);
    } else {
        report.exact = false;
        let show: Vec<_> = diverged.iter().take(8).copied().collect();
        report.summary = format!(
            "exactness: FAIL — stdout diverged vs {} for: {}{}",
            report.targets[0].target,
            show.join(", "),
            if diverged.len() > 8 {
                format!(" (+{} more)", diverged.len() - 8)
            } else {
                String::new()
            }
        );
    }
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
        println!("  front-end  FAIL  {}", report.front_err.as_deref().unwrap_or(""));
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

pub fn receipt_json(report: &CheckReport) -> String {
    let mut seats = String::from("[");
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
    }
    seats.push(']');
    format!(
        "{{\n  \"path\": {:?},\n  \"source_hash\": {:?},\n  \"exact\": {},\n  \"summary\": {:?},\n  \"langs\": {},\n  \"seats\": {}\n}}\n",
        report.path.display().to_string(),
        report.source_hash,
        report.exact,
        report.summary,
        report.targets.len(),
        seats
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_names_the_program_not_the_path() {
        let src = "fn main() { say(1) }\n";
        let report = CheckReport {
            path: PathBuf::from("ignored.cuni"),
            source_hash: said::said(src),
            front_ok: true,
            front_err: None,
            targets: vec![],
            exact: true,
            summary: "exactness: PASS (0 langs)".into(),
        };
        let rec = receipt_json(&report);
        assert!(rec.contains("source_hash"));
        assert!(rec.contains(&said::said(src)));
        assert_ne!(said::said(src), said::said("fn main() { say(2) }\n"));
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
