//! Cross-chain exactness proof profile ("replace trust with proof").
//!
//! `examples/proof-crosschain/escrow.cuni` states an escrow transfer-validation
//! law once; this suite gates it the way the profile promises:
//!
//! 1. **Behavioral exactness** — `cuni check --only rs,go,py` emits the
//!    fixture and runs each artifact with that target's OWN toolchain
//!    (`rustc`, `go run`, `python3`), requiring byte-identical stdout and
//!    `exactness: PASS` on exit 0.
//! 2. **Deployability** — `cuni check --only sol` emits a real Solidity
//!    contract and compiles it with the real `solc` 0.8.28 (a contract has
//!    no stdout, so successful compilation to bytecode IS the sol
//!    verification). The emitted `.sol` artifact is also asserted to exist.
//! 3. **Pinned verdicts** — the six driver outputs are asserted exactly, so
//!    the validation law can't silently change under the gate.
//!
//! No mocked toolchains, no skipped asserts. On-chain execution is out of
//! scope (see the README); everything asserted here really runs.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/proof-crosschain/escrow.cuni";
/// The six driver verdicts, pinned: valid / fee / insufficient /
/// bad-amount / bad-fee-rate / exact-boundary balance.
const EXPECTED_VERDICTS: &str = "0\n25\n1\n2\n3\n0\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. `cuni check` spawns
/// `rustc`, `go`, and `solc` directly, so the test prefixes them onto PATH
/// for the child process (grandchildren inherit it).
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/toolchains/go/bin"),
        format!("{home}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"),
        format!("{home}/toolchains/bin"),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

fn workdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cuni_proof_crosschain_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
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

#[test]
fn escrow_sol_compiles_to_deployable_bytecode() {
    let dir = workdir();
    let sol_path = dir.join("escrow.sol");

    // 1. The sol seat: emit + real solc 0.8.28 compile inside `cuni check`.
    //    Exit 0 means solc accepted the contract (deployable).
    let (ok, stdout, stderr) =
        run_cuni(&["check", FIXTURE, "--only", "sol", "--timeout", "180"]);
    assert!(
        ok,
        "sol seat failed — the contract did not compile under real solc\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // 2. The emitted .sol artifact really exists and is a real contract.
    let (ok, _, stderr) = run_cuni(&[
        FIXTURE,
        "--emit-sol",
        sol_path.to_str().expect("non-utf8 temp path"),
    ]);
    assert!(ok, "--emit-sol failed\n{stderr}");
    assert!(
        sol_path.is_file(),
        "emitted .sol artifact missing at {}",
        sol_path.display()
    );
    let sol_src = std::fs::read_to_string(&sol_path).expect("failed to read emitted .sol");
    assert!(
        sol_src.contains("contract Escrow"),
        "emitted .sol has no contract body"
    );
    assert!(
        sol_src.contains("function validate_transfer"),
        "emitted .sol lost the validation law"
    );

    // 3. Clean up the temp work dir.
    std::fs::remove_dir_all(&dir).expect("failed to clean up temp work dir");
    assert!(!dir.exists(), "temp work dir was not removed");
}
