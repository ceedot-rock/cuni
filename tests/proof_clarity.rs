//! Clarity contract proof profile ("Trust Provable, in all things").
//!
//! `examples/onchain/clarity/demo.cuni` states a small int/dec/str/bool law
//! once; this suite gates it the way the profile promises:
//!
//! 1. **Pinned verdicts** — `cuni run` prints the driver outputs, asserted
//!    byte-for-byte (the law itself must not silently change).
//! 2. **Contract shape** — `cuni compile --emit-clarity` emits a genuine
//!    Clarity contract: `define-private` logic helpers, `define-public`
//!    `(ok ...)` wrappers, and the two delimited regions, in order.
//! 3. **Golden snapshot** — the full emitted program is byte-identical to
//!    `tests/snapshots/clarity_demo.clar.snap`; drift fails loudly.
//! 4. **Logic-core gate** — `--emit-clarity-ref` emits the standalone Python
//!    reference; `python3` runs it and stdout must be byte-identical to the
//!    CuNi gold (`cuni run` on the same fixture).
//! 5. **Money bridge** — `examples/finance/fee_schedule.cuni` (tiered fees
//!    on `dec`, six driver lines) goes through emit + reference; the
//!    reference stdout is byte-identical to the CuNi gold, pinned.
//! 6. **Refusals** — float, list, `??`, `mut`, `for`, `dec %`, and unknown
//!    calls refuse with clear messages.
//! 7. **Toolchain honesty** — `python3` is verified present before the gate
//!    is claimed; `clarity-cli` is checked and expected absent (no Clarity
//!    toolchain on the check machine), in which case the suite asserts shape
//!    + snapshot + reference only.
//!
//! Honest boundaries (see also docs/ONCHAIN.md): the Python reference is
//! gate-proven; the Clarity contract shell is NOT compiled here; nothing has
//! executed on-chain. No mocked toolchains, no skipped asserts.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/onchain/clarity/demo.cuni";
const SNAPSHOT: &str = "tests/snapshots/clarity_demo.clar.snap";
/// The eleven driver outputs, pinned (computed honestly via `cuni run`).
const EXPECTED: &str = "-2\n-2\n-1\n1.2499\n1.5\ntier-one\ntier-two\nTrue\nFalse\n1\n3.5\n";
const FEE_FIXTURE: &str = "examples/finance/fee_schedule.cuni";
/// The six tiered-fee driver outputs, pinned (computed honestly via `cuni run`).
const FEE_EXPECTED: &str = "1.2499\n1.25\n5.25\n2.75\n0.25\n2500.25\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. `cargo` lives under
/// `~/.cargo/bin` here; prefix every plausible location onto PATH for the
/// child processes (grandchildren inherit it).
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [format!("{home}/go/bin"), format!("{home}/.cargo/bin")];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

/// Unique-per-test work dir: tests in one binary run on parallel threads
/// sharing a process id, so the dir name includes the test's tag.
fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cuni_proof_clarity_{}_{}", tag, std::process::id()));
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

/// The interpreter's stdout for a fixture: the gold every reference must match.
fn cuni_gold(fixture: &str) -> String {
    let (ok, stdout, stderr) = run_cuni(&["run", fixture]);
    assert!(
        ok,
        "cuni run {fixture} failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    stdout
}

fn write_fixture(dir: &PathBuf, name: &str, src: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).expect("failed to write temp fixture");
    p
}

fn utf8(p: &PathBuf) -> &str {
    p.to_str().expect("non-utf8 temp path")
}

#[test]
fn demo_verdicts_are_pinned() {
    // The law itself must not silently change: the interpreter prints the
    // eleven driver outputs, asserted byte-for-byte.
    let gold = cuni_gold(FIXTURE);
    assert_eq!(
        gold, EXPECTED,
        "demo driver outputs drifted — the law changed without the gate catching it"
    );
}

#[test]
fn clarity_contract_shape() {
    let dir = workdir("shape");
    let prog_path = dir.join("demo.clar");
    let (ok, _, stderr) = run_cuni(&["compile", FIXTURE, "--emit-clarity", utf8(&prog_path)]);
    assert!(ok, "--emit-clarity failed\n{stderr}");
    assert!(prog_path.is_file(), "emitted contract artifact missing");
    let src = std::fs::read_to_string(&prog_path).expect("failed to read emitted contract");

    // Both regions delimited, in order: logic first, shell second.
    let markers = [
        ";; CUNI-LOGIC-CORE-START",
        ";; CUNI-LOGIC-CORE-END",
        ";; CUNI-CLARITY-SHELL-START",
        ";; CUNI-CLARITY-SHELL-END",
    ];
    let mut prev = 0;
    for m in markers {
        let at = src.find(m).unwrap_or_else(|| panic!("contract missing `{m}`"));
        assert!(at >= prev, "contract markers out of order at `{m}`");
        prev = at;
    }
    // Idiomatic Clarity shape: private logic helpers, public `(ok ...)` surface.
    for marker in [
        "(define-private (trunc-div-impl (a int) (b int))",
        "(define-private (fee-impl (amount int))",
        "(define-public (fee (amount int))",
        "(ok (fee-impl amount))",
        "(define-public (cuni-driver)",
        "(print (fee-impl 999999))",
        ";; Contract: cuni-demo",
    ] {
        assert!(src.contains(marker), "contract missing `{marker}`");
    }
    // The `dec` law is visible in scaled form: 100.00dec -> 1000000,
    // 0.01dec -> 100, 0.25dec -> 2500.
    assert!(
        src.contains("(/ (* amount 100) 10000)"),
        "dec multiplication must use the scaled form"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn golden_snapshot_matches() {
    // The full emitted program must be byte-identical to the golden
    // snapshot; any drift fails loudly here.
    let dir = workdir("snapshot");
    let prog_path = dir.join("demo.clar");
    let (ok, _, stderr) = run_cuni(&["compile", FIXTURE, "--emit-clarity", utf8(&prog_path)]);
    assert!(ok, "--emit-clarity failed\n{stderr}");
    let emitted = std::fs::read_to_string(&prog_path).expect("failed to read emitted contract");
    let snap_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
    let snap = std::fs::read_to_string(&snap_path).expect("failed to read golden snapshot");
    assert_eq!(
        emitted, snap,
        "emitted Clarity program drifted from the golden snapshot {SNAPSHOT} — \
         re-verify the diff by hand and update the snapshot only if the change is intended"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reference_gate_python() {
    // The standalone Python reference runs under `python3` and its stdout
    // must be byte-identical to the CuNi gold.
    let dir = workdir("refgate");
    let ref_path = dir.join("demo_ref.py");
    let (ok, _, stderr) = run_cuni(&["compile", FIXTURE, "--emit-clarity-ref", utf8(&ref_path)]);
    assert!(ok, "--emit-clarity-ref failed\n{stderr}");
    assert!(ref_path.is_file(), "emitted reference artifact missing");
    let run = Command::new("python3")
        .arg(utf8(&ref_path))
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to spawn python3");
    assert!(
        run.status.success(),
        "reference failed to run\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let gold = cuni_gold(FIXTURE);
    assert_eq!(
        stdout, gold,
        "reference stdout diverged from the CuNi gold"
    );
    assert_eq!(stdout, EXPECTED, "reference stdout diverged from the pinned verdicts");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn money_bridge_fee_schedule() {
    // The tiered-fee law on `dec` (six driver lines) goes through the full
    // Clarity profile: contract shape + Python reference == CuNi gold.
    let dir = workdir("money");
    let prog_path = dir.join("fee.clar");
    let (ok, _, stderr) = run_cuni(&[
        "compile",
        FEE_FIXTURE,
        "--emit-clarity",
        utf8(&prog_path),
    ]);
    assert!(ok, "--emit-clarity on fee_schedule failed\n{stderr}");
    let src = std::fs::read_to_string(&prog_path).expect("failed to read fee contract");
    for marker in [
        ";; CUNI-LOGIC-CORE-START",
        ";; CUNI-CLARITY-SHELL-START",
        "(define-private (fee-impl (amount int))",
        "(define-public (fee (amount int))",
    ] {
        assert!(src.contains(marker), "fee contract missing `{marker}`");
    }

    let ref_path = dir.join("fee_ref.py");
    let (ok, _, stderr) = run_cuni(&[
        "compile",
        FEE_FIXTURE,
        "--emit-clarity-ref",
        utf8(&ref_path),
    ]);
    assert!(ok, "--emit-clarity-ref on fee_schedule failed\n{stderr}");
    let run = Command::new("python3")
        .arg(utf8(&ref_path))
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to spawn python3");
    assert!(
        run.status.success(),
        "fee reference failed to run\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let gold = cuni_gold(FEE_FIXTURE);
    assert_eq!(stdout, gold, "fee reference stdout diverged from the CuNi gold");
    assert_eq!(
        stdout, FEE_EXPECTED,
        "fee driver outputs drifted — the money law changed"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn refusals_are_honest() {
    // Everything outside the v1 subset refuses with a clear message —
    // through the real CLI, not a unit-test shortcut.
    let dir = workdir("refusals");
    let cases: &[(&str, &str)] = &[
        (
            "float",
            "def f() -> int do\n ret 1\nend\nsay(1.5)\n",
        ),
        (
            "list",
            "def f() -> int do\n let xs = [1, 2]\n ret 1\nend\nsay(f())\n",
        ),
        (
            "unwrap",
            "def f() -> int do\n ret 1\nend\ndef g() -> int do\n ret f() ?? do\n ret 0\nend\nend\nsay(g())\n",
        ),
        (
            "mut",
            "def f() -> int do\n mut x: int = 1\n ret x\nend\nsay(f())\n",
        ),
        (
            "for",
            "def f() -> int do\n for i in range(3) do\n say(i)\n end\n ret 1\nend\nsay(f())\n",
        ),
        (
            "dec-mod",
            "def f(d: dec) -> dec do\n ret d % 2.0dec\nend\nsay(f(1.0dec))\n",
        ),
        (
            "unknown-call",
            "def f() -> int do\n ret 1\nend\nsay(nope(1))\n",
        ),
        (
            "struct",
            "typ Point do\n x: int\nend\ndef f() -> int do\n ret 1\nend\nsay(f())\n",
        ),
    ];
    for (tag, src) in cases {
        let fixture = write_fixture(&dir, &format!("refuse_{tag}.cuni"), src);
        let out = dir.join(format!("refuse_{tag}.clar"));
        let (ok, _, stderr) =
            run_cuni(&["compile", utf8(&fixture), "--emit-clarity", utf8(&out)]);
        assert!(!ok, "{tag}: emit should have refused");
        // Two honest refusal layers: CuNi's own typeck (`type error`) runs
        // before emit, and the emitter itself refuses (`refused`). Either is
        // a clear refusal; silent acceptance is what must never happen.
        assert!(
            stderr.contains("refused") || stderr.contains("type error"),
            "{tag}: refusal needs a clear message, got:\n{stderr}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn toolchain_honesty() {
    let path = toolchain_path();
    // python3 must exist — the reference gate depends on it.
    let py = Command::new("python3")
        .arg("--version")
        .env("PATH", &path)
        .output()
        .expect("failed to spawn python3");
    assert!(
        py.status.success(),
        "python3 must be present to claim the reference gate"
    );
    // The Clarity toolchain is expected absent on the check machine.
    let which = Command::new("sh")
        .args(["-c", "command -v clarity-cli"])
        .env("PATH", &path)
        .output()
        .expect("failed to spawn sh");
    if which.status.success() {
        // A future machine WITH the toolchain should actually check the artifact.
        let dir = workdir("toolchain");
        let prog_path = dir.join("demo.clar");
        let (ok, _, stderr) =
            run_cuni(&["compile", FIXTURE, "--emit-clarity", utf8(&prog_path)]);
        assert!(ok, "--emit-clarity failed\n{stderr}");
        let check = Command::new("clarity-cli")
            .args(["check", utf8(&prog_path)])
            .env("PATH", &path)
            .output()
            .expect("failed to spawn clarity-cli");
        assert!(
            check.status.success(),
            "clarity-cli check failed on the emitted contract\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&check.stdout),
            String::from_utf8_lossy(&check.stderr)
        );
        std::fs::remove_dir_all(&dir).ok();
    } else {
        eprintln!(
            "note: no `clarity-cli` on this machine — contract shell not compiled here \
             (shape + snapshot + reference gate only); nothing has executed on-chain"
        );
    }
}
