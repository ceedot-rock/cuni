//! Solana program proof profile ("Trust Provable, in all things").
//!
//! `examples/proof-solana/escrow.cuni` states a transfer-validation law once;
//! this suite gates it the way the profile promises:
//!
//! 1. **Behavioral exactness** — `cuni check --only rs,go,py` emits the
//!    fixture and runs each artifact with that target's OWN toolchain
//!    (`rustc`, `go run`, `python3`), requiring byte-identical stdout and
//!    `exactness: PASS` on exit 0. The six driver verdicts are pinned.
//! 2. **Program shape** — `cuni --emit-solana` emits a real Anchor-shaped
//!    Solana program: `declare_id!`, `#[program]`, a `#[derive(Accounts)]`
//!    accounts struct, one instruction per CuNi function, and the two
//!    delimited regions (`CUNI-LOGIC-CORE-*` / `CUNI-SOLANA-SHELL-*`).
//! 3. **Logic-core chain** — the delimited logic module is extracted from
//!    the emitted program, compiled STANDALONE with plain `rustc` (no
//!    dependencies — the shell is excluded), run, and its stdout is asserted
//!    byte-identical to the pinned verdicts. The exact code that would ship
//!    inside the on-chain program is the code the gate ran.
//!
//! Honest boundaries (see also docs/SOLANA.md): the logic core is
//! gate-proven; the program shell is NOT compiled here (no Solana toolchain
//! on this machine, no `anchor-lang` vendored); nothing has executed
//! on-chain. No mocked toolchains, no skipped asserts.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/proof-solana/escrow.cuni";
/// The six driver verdicts, pinned: valid / fee / insufficient /
/// bad-amount / bad-fee-rate / exact-boundary balance.
const EXPECTED_VERDICTS: &str = "0\n25\n1\n2\n3\n0\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. `cuni check` spawns
/// `rustc` and `go` directly, and this suite compiles the extracted logic
/// core with `rustc` itself, so the test prefixes every plausible location
/// onto PATH for the child processes (grandchildren inherit it).
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/toolchains/go/bin"),
        format!("{home}/go/bin"),
        format!("{home}/.cargo/bin"),
        format!("{home}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"),
        format!("{home}/toolchains/bin"),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

fn workdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cuni_proof_solana_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create temp work dir");
    dir
}

fn run_cuni(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(cuni_bin())
        .args(args)
        .env("PATH", toolchain_path())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to spawn cuni binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn escrow_exactness_gate_rs_go_py() {
    // Behavioral exactness: real rustc / go run / python3, byte-identical
    // stdout, or the gate refuses.
    let (ok, stdout, stderr) =
        run_cuni(&["check", FIXTURE, "--only", "rs,go,py", "--timeout", "120"]);
    assert!(
        ok,
        "exactness gate failed for rs,go,py\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "missing 'exactness: PASS'\n{stdout}"
    );
}

#[test]
fn escrow_verdicts_are_pinned() {
    // The law itself must not silently change: the interpreter seat prints
    // the six driver verdicts, asserted byte-for-byte.
    let (ok, stdout, stderr) = run_cuni(&["run", FIXTURE]);
    assert!(
        ok,
        "cuni run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout, EXPECTED_VERDICTS,
        "validation verdicts drifted — the law changed without the gate catching it"
    );
}

/// Extract the delimited logic module from a `--emit-solana` artifact,
/// unwrap `mod logic { ... }`, and dedent one level, yielding the
/// standalone logic core.
fn extract_logic_core(program_src: &str) -> String {
    let start_marker = "// CUNI-LOGIC-CORE-START";
    let end_marker = "// CUNI-LOGIC-CORE-END";
    let start = program_src
        .find(start_marker)
        .expect("program missing CUNI-LOGIC-CORE-START");
    let end = program_src
        .find(end_marker)
        .expect("program missing CUNI-LOGIC-CORE-END");
    assert!(start < end, "logic markers out of order");
    let block = &program_src[start..end];
    let mut lines = block.lines().skip(1).peekable();
    // First line inside the region must open the module.
    let open = lines.next().expect("empty logic region");
    assert_eq!(
        open.trim(),
        "mod logic {",
        "logic region must open with `mod logic {{`"
    );
    let mut out = String::new();
    for line in lines {
        if line == "}" {
            // The module's closing brace (indent 0, last line of region).
            break;
        }
        let dedented = line.strip_prefix("    ").unwrap_or(line);
        out.push_str(dedented);
        out.push('\n');
    }
    assert!(
        out.contains("pub fn validate_transfer"),
        "extracted logic core lost the validation law"
    );
    out
}

#[test]
fn escrow_solana_program_shape_and_standalone_logic_core() {
    let dir = workdir();
    let prog_path = dir.join("escrow_program.rs");
    let core_path = dir.join("logic_core.rs");
    let bin_path = dir.join("logic_core_bin");

    // 1. Emit the full Anchor-shaped program.
    let (ok, _, stderr) = run_cuni(&[
        FIXTURE,
        "--emit-solana",
        prog_path.to_str().expect("non-utf8 temp path"),
    ]);
    assert!(ok, "--emit-solana failed\n{stderr}");
    assert!(prog_path.is_file(), "emitted program artifact missing");
    let src = std::fs::read_to_string(&prog_path).expect("failed to read emitted program");

    // 2. The program shell is Anchor-shaped and both regions are delimited.
    for marker in [
        "// CUNI-LOGIC-CORE-START",
        "// CUNI-LOGIC-CORE-END",
        "// CUNI-SOLANA-SHELL-START",
        "// CUNI-SOLANA-SHELL-END",
        "use anchor_lang::prelude::*;",
        "declare_id!",
        "#[program]",
        "pub mod cuni_escrow",
        "#[derive(Accounts)]",
        "pub struct Run<'info>",
        "pub signer: Signer<'info>,",
        "logic::validate_transfer(",
    ] {
        assert!(src.contains(marker), "program shell missing `{}`", marker);
    }

    // 3. Extract the logic core and compile it STANDALONE with plain rustc
    //    (no anchor-lang, no Solana toolchain — the shell is excluded by
    //    construction). This is the most this machine can honestly verify.
    let core = extract_logic_core(&src);
    std::fs::write(&core_path, &core).expect("failed to write extracted logic core");
    let compile = Command::new("rustc")
        .args([
            "-O",
            "-o",
            bin_path.to_str().expect("non-utf8 temp path"),
            core_path.to_str().expect("non-utf8 temp path"),
        ])
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to spawn rustc");
    assert!(
        compile.status.success(),
        "extracted logic core did not compile with plain rustc\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&bin_path)
        .output()
        .expect("failed to run compiled logic core");
    assert!(run.status.success(), "compiled logic core failed to run");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert_eq!(
        stdout, EXPECTED_VERDICTS,
        "extracted logic core stdout diverged from the pinned verdicts"
    );

    // 4. Clean up the temp work dir.
    std::fs::remove_dir_all(&dir).expect("failed to clean up temp work dir");
    assert!(!dir.exists(), "temp work dir was not removed");
}
