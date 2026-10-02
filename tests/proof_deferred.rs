//! Proof: deferred seats are full native seats (examples/proof-deferred/core.cuni).
//!
//! The 0.9.0 changelog deferred clj/ex/hs (emitter bugs), pro/erl
//! (infeasible), tcl (broken spec). This test proves otherwise: each of the
//! six emits real code through its own toolchain (clojure, elixir, runghc,
//! swipl, escript, tclsh) and produces byte-identical stdout to the Python
//! gold on the full core subset — say/let/mut, def/ret incl. recursion,
//! if/els, whl, int arithmetic (truncating `/`, Python-floored `%`),
//! comparisons, and/or/not, unary minus, string concat and escapes.
//! Anything beyond the core subset refuses honestly; nothing is approximated.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Runs `cuni check` on the deferred fixture for one seat and asserts the
/// exactness gate passes.
fn assert_seat(seat: &str) {
    let output = Command::new(cuni_bin())
        .arg("check")
        .arg("examples/proof-deferred/core.cuni")
        .arg("--only")
        .arg(seat)
        .arg("--timeout")
        .arg("120")
        .output()
        .expect("failed to invoke cuni check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "exactness gate failed for seat {seat} (exit {}):\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code().unwrap_or(-1),
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "expected 'exactness: PASS' for seat {seat} in:\n{stdout}",
    );
}

#[test]
fn deferred_clj_exactness() {
    assert_seat("clj");
}

#[test]
fn deferred_ex_exactness() {
    assert_seat("ex");
}

#[test]
fn deferred_hs_exactness() {
    assert_seat("hs");
}

#[test]
fn deferred_pro_exactness() {
    assert_seat("pro");
}

#[test]
fn deferred_erl_exactness() {
    assert_seat("erl");
}

#[test]
fn deferred_tcl_exactness() {
    assert_seat("tcl");
}

/// All six together plus Python: one gate, byte-identical everywhere.
#[test]
fn deferred_all_six_plus_py() {
    let output = Command::new(cuni_bin())
        .arg("check")
        .arg("examples/proof-deferred/core.cuni")
        .arg("--only")
        .arg("py,clj,ex,hs,pro,erl,tcl")
        .arg("--timeout")
        .arg("300")
        .output()
        .expect("failed to invoke cuni check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "combined exactness gate failed (exit {}):\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code().unwrap_or(-1),
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "expected 'exactness: PASS' in:\n{stdout}",
    );
}
