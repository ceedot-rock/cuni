//! Proof-crypto conformance: the "replace trust with proof" gate for
//! multi-implementation cryptography.
//!
//! Runs `cuni check` over `examples/proof-crypto/digest.cuni` on the three
//! real native seats (rs, go, py) and asserts the gate passes — i.e. three
//! independently generated implementations of the FNV-1a reference digest
//! (plus the toy RSA verify) produce byte-identical stdout. A second step
//! runs the emitted Python seat directly and asserts the exact gold digest
//! values, which were computed independently (python3 one-liner, not the
//! CuNi compiler) and are also asserted in-source in digest.cuni.
//!
//! Toolchain dirs are not on the default PATH, so PATH is extended for the
//! child commands. No mocked toolchains, no skipped asserts.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// PATH with the real Go and Rust toolchains prepended.
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set");
    let go_bin = format!("{}/toolchains/go/bin", home);
    let rs_bin = format!(
        "{}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin",
        home
    );
    let old = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}:{}", go_bin, rs_bin, old)
}

/// Gold stdout: FNV-1a 32 of "hello", "", "foobar", then the RSA verify
/// result (powmod(588, 17, 3233) == 65). Computed independently via:
///   python3 -c "def f(bs):
///       h=2166136261
///       for b in bs: h=((h^b)*16777619)&0xFFFFFFFF
///       return h
///   print(f(b'hello')); print(f(b'')); print(f(b'foobar')); print(pow(588,17,3233))"
const GOLD: &str = "1335831723\n2166136261\n3214735720\n65\n";

#[test]
fn proof_crypto_digest_exactness() {
    let path = toolchain_path();
    let output = Command::new(cuni_bin())
        .args([
            "check",
            "examples/proof-crypto/digest.cuni",
            "--only",
            "rs,go,py",
            "--timeout",
            "120",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", &path)
        .output()
        .expect("failed to invoke cuni binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cuni check failed:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "stdout missing 'exactness: PASS':\n{}",
        stdout
    );
}

#[test]
fn proof_crypto_gold_digests() {
    let path = toolchain_path();
    let dir = std::env::temp_dir().join(format!("cuni_proof_crypto_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let py_out = dir.join("digest.py");

    // Emit the Python seat and run it with the real interpreter.
    let emit = Command::new(cuni_bin())
        .arg("examples/proof-crypto/digest.cuni")
        .arg("--emit-py")
        .arg(&py_out)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", &path)
        .output()
        .expect("failed to invoke cuni binary");
    assert!(
        emit.status.success(),
        "cuni --emit-py failed:\n{}",
        String::from_utf8_lossy(&emit.stderr)
    );

    let run = Command::new("python3")
        .arg(&py_out)
        .env("PATH", &path)
        .output()
        .expect("failed to run python3");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "python3 seat failed:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        stdout.as_ref(),
        GOLD,
        "digest output diverged from independently computed gold values"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
