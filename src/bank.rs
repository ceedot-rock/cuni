//! CuNi Bank — paste N, get X. Ingest → emit → prove, or refuse.

use crate::check;
use crate::emit;
use crate::ingest;
use crate::langs;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

pub fn cmd_bank(args: &[String]) -> ExitCode {
    if args.is_empty() || args[0] == "--help" {
        eprintln!(
            "cuni bank paste <file> --from py --to <id> [-o out]\n\
             Arm of CuNi. See docs/BANK.md."
        );
        return ExitCode::FAILURE;
    }
    if args[0] != "paste" {
        eprintln!("cuni bank: unknown `{}` — want `paste`", args[0]);
        return ExitCode::FAILURE;
    }
    let mut input = None;
    let mut from = None;
    let mut to = None;
    let mut output = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => { from = args.get(i + 1).cloned(); i += 2; }
            "--to" => { to = args.get(i + 1).cloned(); i += 2; }
            "-o" | "--output" => { output = args.get(i + 1).cloned(); i += 2; }
            s if s.starts_with('-') => {
                eprintln!("cuni bank: unknown flag `{s}`");
                return ExitCode::FAILURE;
            }
            _ => { input = Some(args[i].clone()); i += 1; }
        }
    }
    let Some(input) = input else {
        eprintln!("cuni bank paste: missing file");
        return ExitCode::FAILURE;
    };
    let from = from.unwrap_or_else(|| "py".into());
    let Some(to) = to else {
        eprintln!("cuni bank paste: --to <id> required");
        return ExitCode::FAILURE;
    };
    if from != "py" && from != "cuni" {
        eprintln!("cuni bank: refuse --from {from} — v1 is py|cuni (docs/BANK.md)");
        return ExitCode::FAILURE;
    }
    let Some(lang) = langs::find(&to) else {
        eprintln!("cuni bank: refuse unknown --to `{to}`");
        return ExitCode::FAILURE;
    };
    let src = match fs::read(&input) {
        Ok(b) => b,
        Err(e) => { eprintln!("cuni bank: {e}"); return ExitCode::FAILURE; }
    };
    let hash = source_hash(&src);
    match ingest::ingest_file(Path::new(&input)) {
        Err(e) => { eprintln!("cuni bank: ingest refuse\n{e}"); ExitCode::FAILURE }
        Ok(cuni_src) => {
            let work = std::env::temp_dir().join(format!("cuni_bank_{}", std::process::id()));
            let _ = fs::create_dir_all(&work);
            let cuni_path = work.join("deposit.cuni");
            if let Err(e) = fs::write(&cuni_path, &cuni_src) {
                eprintln!("cuni bank: {e}");
                return ExitCode::FAILURE;
            }
            let program = match check::load_program(&cuni_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("cuni bank: refuse — ingested CuNi failed\n{e}");
                    return ExitCode::FAILURE;
                }
            };
            let artifact = emit::generate_exact(&program, lang);
            let ext = lang.ext;
            let out_path = output.map(PathBuf::from).unwrap_or_else(|| work.join(format!("emit.{ext}")));
            if let Err(e) = fs::write(&out_path, &artifact) {
                eprintln!("cuni bank: {e}");
                return ExitCode::FAILURE;
            }
            let report = check::check_file_only(
                &cuni_path, &work, Duration::from_secs(60), Some(&["py".to_string()]),
            );
            if !report.passed() {
                eprintln!("cuni bank: refuse — gold exactness failed\n{}", report.summary);
                return ExitCode::FAILURE;
            }
            let Some(gold) = check::gold_stdout(&report).map(|s| s.to_string()) else {
                eprintln!("cuni bank: refuse — no gold stdout");
                return ExitCode::FAILURE;
            };
            if let Err(e) = prove_seat(lang, &out_path, &gold) {
                eprintln!("cuni bank: prove refuse\n{e}");
                return ExitCode::FAILURE;
            }
            println!("bank: PASS — from={from} to={} out={} source_hash={hash}", lang.id, out_path.display());
            ExitCode::SUCCESS
        }
    }
}

fn prove_seat(lang: &langs::Lang, artifact: &Path, gold: &str) -> Result<(), String> {
    let plan = emit::exec_plan(lang, artifact);
    if let Some((cmd, args)) = &plan.compile {
        run_plan(cmd, args)?;
    }
    let got = run_plan(&plan.run.0, &plan.run.1)?;
    if got != gold {
        return Err(format!("stdout mismatch\n--- gold ---\n{gold}--- got ---\n{got}"));
    }
    Ok(())
}

fn run_plan(cmd: &str, args: &[String]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{cmd} exited {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn source_hash(bytes: &[u8]) -> String {
    let mut h = 0xcbf29ce484222325u64;
    for &b in bytes { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); }
    format!("{h:016x}")
}
