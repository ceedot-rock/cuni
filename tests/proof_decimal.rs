//! Decimal exactness proof profile ("Trust Provable, in all things").
//!
//! `examples/proof-decimal/money.cuni` states one money law once — fees,
//! splits, truncated division, comparisons, explicit conversions, canonical
//! printing — and this suite gates it the way the profile promises:
//!
//! 1. **Behavioral exactness** — `cuni check --only py,go,js,ts,c,cpp,rs,rb,
//!    lua,java,sql` emits the fixture and runs each artifact with that
//!    target's OWN toolchain, requiring byte-identical stdout and
//!    `exactness: PASS` on exit 0. Inexact seats don't get to silently
//!    round: they refuse, and refusals are tested below.
//! 2. **Pinned verdicts** — the 23 driver outputs are asserted exactly, so
//!    the money law can't silently change under the gate.
//! 3. **Deployability** — `cuni check --only sol` emits a real Solidity
//!    contract and compiles it with the real `solc` (a contract has no
//!    stdout, so successful compilation to bytecode IS the sol
//!    verification).
//! 4. **Refusals** — out-of-range dec literals fail loudly on the narrow
//!    (int64) seats go/lua/sql; mixed dec/int arithmetic, `%` on dec,
//!    division by zero, and >4 fractional digits are refused at the
//!    front-end or at run time — never silently computed.
//!
//! No mocked toolchains, no skipped asserts. Everything asserted here
//! really runs.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/proof-decimal/money.cuni";
/// The 23 driver outputs, pinned: fee / split / neg-div x2 / trunc-mul x2 /
/// add / sub / neg / comparisons x3 / conversions x4 / print forms x5 /
/// interpolation.
const EXPECTED_VERDICTS: &str = "25.0\n33.3333\n-3.75\n-0.6172\n0.0999\n1.0002\n10.75\n10.25\n-5.5\nTrue\nTrue\nTrue\n5.0\n-3.0\n19\n-19\n1.23\n100.0\n0.0001\n-0.0005\n0.0\nfee on 5000.00 at 0.50%: 25.0\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. `cuni check` spawns
/// `rustc`, `go`, `javac`, and `solc` directly, so the test prefixes them
/// onto PATH for the child process (grandchildren inherit it).
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/go/bin"),
        format!("{home}/toolchains/go/bin"),
        format!("{home}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"),
        format!("{home}/toolchains/bin"),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

fn workdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cuni_proof_decimal_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create temp work dir");
    dir
}

/// Unique scratch dir per call: tests run in parallel threads of one
/// process, so the process id alone is not unique.
fn scratch_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "cuni_proof_decimal_{}_{}_{:?}",
        std::process::id(),
        n,
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp work dir");
    dir
}

fn run_cuni(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(cuni_bin())
        .args(args)
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to spawn cuni binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Write a scratch fixture, run `args` against it, return the outcome.
fn run_scratch(src: &str, args: &[&str]) -> (bool, String, String) {
    let dir = scratch_dir();
    let path = dir.join("scratch.cuni");
    std::fs::write(&path, src).expect("failed to write scratch fixture");
    let mut full: Vec<&str> = vec!["check", path.to_str().expect("non-utf8 temp path")];
    full.extend_from_slice(args);
    let out = run_cuni(&full);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn money_exactness_gate_all_native_seats() {
    // Behavioral exactness: every native seat's own toolchain, byte-identical
    // stdout, or the gate refuses. sol is compile-only (separate test).
    let (ok, stdout, stderr) = run_cuni(&[
        "check",
        FIXTURE,
        "--only",
        "py,go,js,ts,c,cpp,rs,rb,lua,java,sql",
        "--timeout",
        "300",
    ]);
    assert!(
        ok,
        "exactness gate failed for the 11 dec seats\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "missing 'exactness: PASS'\n{stdout}"
    );
}

#[test]
fn money_verdicts_are_pinned() {
    // The money law itself must not silently change: the interpreter seat
    // prints the 23 driver outputs, asserted byte-for-byte.
    let (ok, stdout, stderr) = run_cuni(&["run", FIXTURE]);
    assert!(
        ok,
        "cuni run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout, EXPECTED_VERDICTS,
        "money verdicts drifted — the law changed without the gate catching it"
    );
}

#[test]
fn money_sol_compiles_to_deployable_bytecode() {
    // The sol seat: emit + real solc compile inside `cuni check`.
    // Exit 0 means solc accepted the contract (deployable).
    let (ok, stdout, stderr) = run_cuni(&["check", FIXTURE, "--only", "sol", "--timeout", "180"]);
    assert!(
        ok,
        "sol seat failed — the contract did not compile under real solc\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "missing 'exactness: PASS'\n{stdout}"
    );
}

#[test]
fn decimal_narrow_seats_refuse_out_of_range_literal() {
    // 9223372036854775808.0dec scales to 92233720368547758080000, past
    // i64::MAX: the narrow seats (go, lua, sql) must refuse at emit — never
    // wrap. Each seat is checked independently.
    let src = "say(9223372036854775808.0dec)\n";
    for seat in ["go", "lua", "sql"] {
        let (ok, stdout, stderr) = run_scratch(src, &["--only", seat, "--timeout", "120"]);
        let both = format!("{stdout}\n{stderr}");
        assert!(
            !ok,
            "seat {seat} ACCEPTED an out-of-range dec literal — it must refuse\n{both}"
        );
        assert!(
            both.contains("refus"),
            "seat {seat} failed without a refusal message\n{both}"
        );
    }
}

#[test]
fn decimal_mixed_arithmetic_is_a_type_error() {
    // No implicit dec/int conversion: `1.5dec + 1` is a front-end refusal
    // with a fix-it, on every seat (the typeck runs before any emit).
    let (ok, stdout, stderr) = run_scratch(
        "say(1.5dec + 1)\n",
        &["--only", "py,go,js,ts,c,cpp,rs,rb,lua,java,sql", "--timeout", "120"],
    );
    let both = format!("{stdout}\n{stderr}");
    assert!(!ok, "mixed dec/int arithmetic was accepted — it must be refused");
    assert!(
        both.contains("dec_of_int") || both.contains("cannot mix"),
        "mixed dec/int refusal lost its fix-it\n{both}"
    );
}

#[test]
fn decimal_percent_is_refused() {
    // `%` has no meaning on exact decimals: front-end refusal.
    let (ok, stdout, stderr) = run_scratch(
        "say(1.0dec % 2.0dec)\n",
        &["--only", "py", "--timeout", "60"],
    );
    let both = format!("{stdout}\n{stderr}");
    assert!(!ok, "`%` on dec was accepted — it must be refused");
    assert!(
        both.contains('%'),
        "dec `%` refusal message is unclear\n{both}"
    );
}

#[test]
fn decimal_division_by_zero_is_loud() {
    // Division by zero is a loud runtime failure on every seat, never a value.
    let (ok, stdout, stderr) = run_scratch(
        "say(1.0dec / 0.0dec)\n",
        &["--only", "py,go,js,rs,rb,lua,java,sql", "--timeout", "180"],
    );
    let both = format!("{stdout}\n{stderr}");
    assert!(!ok, "dec division by zero was accepted — it must fail loudly");
    assert!(
        both.to_lowercase().contains("zero") || both.contains("refus"),
        "division-by-zero failure has no clear message\n{both}"
    );
}

#[test]
fn decimal_five_fraction_digits_are_refused_at_lex() {
    // >4 fractional digits never silently round: the literal is refused.
    let (ok, stdout, stderr) = run_scratch("say(1.00001dec)\n", &["--only", "py", "--timeout", "60"]);
    let both = format!("{stdout}\n{stderr}");
    assert!(!ok, "a 5-fraction-digit dec literal was accepted — it must be refused");
    assert!(
        both.contains("refus") || both.contains('4'),
        "over-precision refusal message is unclear\n{both}"
    );
}
