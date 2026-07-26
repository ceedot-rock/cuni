//! Exactness check: emit py/go/js, run each, require identical stdout.
//!
//! This is the product surface for SPEC.md §2 — "compile-or-refuse" plus
//! runtime agreement across all v1 targets. Exit status is the platform API:
//! 0 = exactness PASS, 1 = FAIL.

use crate::ast::Program;
use crate::checks;
use crate::codegen_go;
use crate::codegen_js;
use crate::codegen_py;
use crate::lexer::Lexer;
use crate::modules;
use crate::parser::Parser;
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

fn emit_for(program: &Program, target: &str, out: &Path) -> Result<(), String> {
    match target {
        "py" => {
            if let Some(name) = checks::find_ext_collision(program, "py") {
                return Err(format!(
                    "`ext {}` shadows the Python builtin `{}` inside its own py: body",
                    name, name
                ));
            }
            fs::write(out, codegen_py::generate(program)).map_err(|e| e.to_string())
        }
        "go" => fs::write(out, codegen_go::generate(program)).map_err(|e| e.to_string()),
        "js" => {
            if let Some(name) = checks::find_ext_collision(program, "js") {
                return Err(format!(
                    "`ext {}` shadows the JS global `{}` inside its own js: body",
                    name, name
                ));
            }
            fs::write(out, codegen_js::generate(program)).map_err(|e| e.to_string())
        }
        _ => Err(format!("unknown target {}", target)),
    }
}

fn run_target(target: &str, artifact: &Path, timeout: Duration) -> Result<String, String> {
    let (cmd, args): (String, Vec<String>) = match target {
        "py" => (
            "python3".into(),
            vec![artifact.to_string_lossy().into_owned()],
        ),
        "go" => (
            "go".into(),
            vec!["run".into(), artifact.to_string_lossy().into_owned()],
        ),
        "js" => (
            "node".into(),
            vec![artifact.to_string_lossy().into_owned()],
        ),
        _ => return Err(format!("unknown target {}", target)),
    };
    run_blocking(&cmd, &args, timeout)
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
pub fn check_file(path: &Path, work_dir: &Path, timeout: Duration) -> CheckReport {
    let mut report = CheckReport {
        path: path.to_path_buf(),
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
    let targets = ["py", "go", "js"];
    let exts = ["py", "go", "js"];

    for (target, ext) in targets.iter().zip(exts.iter()) {
        let out = work_dir.join(format!("{}.{}", stem, ext));
        let mut tr = TargetResult {
            target,
            emit_ok: false,
            emit_err: None,
            run_ok: false,
            run_err: None,
            stdout: None,
        };
        match emit_for(&program, target, &out) {
            Ok(()) => {
                tr.emit_ok = true;
                match run_target(target, &out, timeout) {
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

    // Exactness: all three emitted, ran, and stdout equal
    let all_ok = report.targets.iter().all(|t| t.emit_ok && t.run_ok);
    if !all_ok {
        let mut parts = Vec::new();
        for t in &report.targets {
            if let Some(e) = &t.emit_err {
                parts.push(format!("{} emit refused: {}", t.target, e));
            } else if let Some(e) = &t.run_err {
                parts.push(format!("{} run failed: {}", t.target, e.lines().next().unwrap_or("")));
            }
        }
        report.summary = format!("exactness: FAIL — {}", parts.join("; "));
        report.exact = false;
        return report;
    }

    let outs: Vec<&str> = report
        .targets
        .iter()
        .map(|t| t.stdout.as_deref().unwrap_or(""))
        .collect();
    if outs[0] == outs[1] && outs[1] == outs[2] {
        report.exact = true;
        report.summary = "exactness: PASS (py/go/js)".into();
    } else {
        report.exact = false;
        report.summary = format!(
            "exactness: FAIL — stdout diverged\n  --- py ---\n{}\n  --- go ---\n{}\n  --- js ---\n{}",
            outs[0], outs[1], outs[2]
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
    for t in &report.targets {
        if !t.emit_ok {
            println!(
                "  emit {:<3}  REFUSE  {}",
                t.target,
                t.emit_err.as_deref().unwrap_or("")
            );
            continue;
        }
        println!("  emit {:<3}  ok", t.target);
        if !t.run_ok {
            println!(
                "  run  {:<3}  FAIL  {}",
                t.target,
                t.run_err.as_deref().unwrap_or("").lines().next().unwrap_or("")
            );
        } else {
            println!("  run  {:<3}  ok", t.target);
            if verbose {
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
