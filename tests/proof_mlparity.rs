//! Proof profile: ML inference parity (examples/proof-mlparity/kernel.cuni).
//!
//! The kernel is a 4x4 integer matrix multiply followed by an argmax over
//! row 0 — the shape of an inference scoring step (logits layer + top-1
//! selection). Integer arithmetic only: cross-language float exactness
//! cannot be guaranteed, so a float kernel is refused rather than faked.
//! See examples/proof-mlparity/README.md for the plain-English version.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// The rust seat shells out to `rustc`, which lives in the rustup toolchain
/// dir — not on the default PATH in this environment. Every child command
/// gets a PATH that includes it.
fn path_with_rustc() -> String {
    let rustup_bin = std::path::PathBuf::from(env!("HOME"))
        .join(".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin");
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", rustup_bin.display(), cur)
}

/// Gold output, computed independently (python3, not the CuNi compiler):
///   a=[3,1,2,0, 0,2,1,4, 1,0,3,2, 2,1,0,3]
///   b=[1,0,2,1, 2,3,0,1, 1,1,2,0, 0,2,1,3]
///   C=A@B rows [7 5 10 4],[5 15 6 14],[4 7 10 7],[4 9 7 12]; argmax(row0)=2.
const EXPECTED: &str = "7 5 10 4\n5 15 6 14\n4 7 10 7\n4 9 7 12\nargmax=2\n";

#[test]
fn mlparity_exactness_gate_passes_py_rs_c() {
    let output = Command::new(cuni_bin())
        .arg("check")
        .arg("examples/proof-mlparity/kernel.cuni")
        .arg("--only")
        .arg("py,rs,c")
        .arg("--timeout")
        .arg("120")
        .env("PATH", path_with_rustc())
        .output()
        .expect("failed to invoke cuni check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "exactness gate failed (exit {}):\nstdout:\n{}\nstderr:\n{}",
        output.status.code().unwrap_or(-1),
        stdout,
        stderr
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "expected 'exactness: PASS' in:\n{}",
        stdout
    );
    // cuni check cleans its own work dir (cuni_check_<pid> under the temp
    // dir) unless --keep, which we did not pass — nothing to clean up here.
}

#[test]
fn mlparity_kernel_computes_expected_scores_and_argmax() {
    // `cuni run` evaluates the kernel; its stdout must equal the gold
    // values computed independently above — including argmax=2.
    let output = Command::new(cuni_bin())
        .args(["run", "examples/proof-mlparity/kernel.cuni"])
        .env("PATH", path_with_rustc())
        .output()
        .expect("failed to invoke cuni run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "cuni run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout.as_ref(),
        EXPECTED,
        "kernel output diverged from independently computed gold"
    );
    assert!(
        stdout.contains("argmax=2"),
        "argmax mismatch in:\n{}",
        stdout
    );
}
