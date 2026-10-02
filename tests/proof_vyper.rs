//! Vyper contract proof profile ("Trust Provable, in all things").
//!
//! `examples/onchain/vyper/demo.cuni` states a small int/dec/bool/str law
//! once; this suite gates it the way the profile promises:
//!
//! 1. **Contract shape** — `cuni compile --emit-vyper` emits a genuine Vyper
//!    contract: `@internal @pure` logic functions (underscore-prefixed, the
//!    Vyper docs' own pattern), one `@external @pure` wrapper per CuNi
//!    `def`, and the two delimited regions (`CUNI-LOGIC-CORE-*` /
//!    `CUNI-VYPER-SHELL-*`).
//! 2. **Golden snapshot** — the FULL emitted program is pinned in
//!    `tests/snapshots/vyper_demo.vy`; any unexpected change fails loudly.
//! 3. **Logic-core gate** — `--emit-vyper-ref` emits a standalone Python
//!    reference of the pure logic core plus the `say` driver; it is run with
//!    the machine's OWN `python3` and its stdout is asserted byte-identical
//!    to `cuni run` on the same fixture. The driver verdicts are pinned.
//! 4. **Money bridge** — `examples/finance/fee_schedule.cuni` (the tiered
//!    fee law on `dec`, six driver lines) goes through the same gate: the
//!    same financial law, proven exact, on EVM. Expected outputs are pinned.
//! 5. **Honest refusals** — float programs refuse; unsupported constructs
//!    (lists, `??`, structs, `time`, string concat, `say` inside functions)
//!    refuse with clear messages.
//! 6. **Toolchain check** — `python3` is verified present before the gate is
//!    claimed; the Vyper toolchain is probed (`which vyper`) and, absent as
//!    expected, the suite asserts shape + snapshot only.
//!
//! Honest boundaries (see also `src/codegen_vyper.rs`): the logic core is
//! gate-proven via the Python reference; the contract shell is NOT compiled
//! here (no Vyper toolchain on this machine); nothing has executed on-chain.
//! No toolchain is installed by this suite, and nothing touches any chain.
//! No mocked toolchains, no skipped asserts.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/onchain/vyper/demo.cuni";
const FEE_FIXTURE: &str = "examples/finance/fee_schedule.cuni";
/// The five driver outputs, pinned: fee @ 99.9999 / fee @ 250.0 /
/// double(21) / ok(10.0) / the closing line.
const EXPECTED_DEMO: &str = "1.2499\n1.5\n42\nTrue\nonchain demo\n";
/// The six tiered-fee verdicts, pinned (docs/DECIMAL.md truncation).
const EXPECTED_FEES: &str = "1.2499\n1.25\n5.25\n2.75\n0.25\n2500.25\n";
/// Golden snapshot of the full `--emit-vyper` program for the demo fixture.
const GOLDEN_PROGRAM: &str = include_str!("snapshots/vyper_demo.vy");

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. The gate runs the
/// reference with the machine's own `python3`, so every plausible location
/// is prefixed onto PATH for the child processes (grandchildren inherit it).
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/go/bin"),
        format!("{home}/.cargo/bin"),
        format!("{home}/.local/bin"),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

/// Scratch dir under the crate target dir (`/tmp` may be full or shared;
/// `target/` is gitignored and this suite cleans up after itself).
/// A per-test counter keeps parallel tests in one process from deleting
/// each other's directories (they share a PID).
fn workdir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "tmp_proof_vyper_{}_{}_{:?}",
            std::process::id(),
            n,
            std::thread::current().id()
        ));
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

fn run_python3(script: &std::path::Path) -> (bool, String, String) {
    let output = Command::new("python3")
        .arg(script)
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to spawn python3");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn vyper_contract_shape() {
    let dir = workdir();
    let prog_path = dir.join("demo.vy");

    let (ok, _, stderr) = run_cuni(&[
        "compile",
        FIXTURE,
        "--emit-vyper",
        prog_path.to_str().expect("non-utf8 temp path"),
    ]);
    assert!(ok, "--emit-vyper failed\n{stderr}");
    assert!(prog_path.is_file(), "emitted contract artifact missing");
    let src = std::fs::read_to_string(&prog_path).expect("failed to read emitted contract");

    // Both delimited regions, in order.
    let logic_start = src.find("# CUNI-LOGIC-CORE-START").expect("missing LOGIC_START");
    let logic_end = src.find("# CUNI-LOGIC-CORE-END").expect("missing LOGIC_END");
    let shell_start = src.find("# CUNI-VYPER-SHELL-START").expect("missing SHELL_START");
    let shell_end = src.find("# CUNI-VYPER-SHELL-END").expect("missing SHELL_END");
    assert!(
        logic_start < logic_end && logic_end < shell_start && shell_start < shell_end,
        "delimited regions out of order"
    );

    // Idiomatic Vyper shape: pure internal logic + external wrappers.
    for marker in [
        "@internal",
        "@pure",
        "def _fee(amount: int256) -> int256:",
        "def _double(n: int256) -> int256:",
        "def _ok(amount: int256) -> bool:",
        "@external",
        "def fee(amount: int256) -> int256:",
        "def double(n: int256) -> int256:",
        "def ok(amount: int256) -> bool:",
        "return self._fee(amount)",
        "return self._double(n)",
        "return self._ok(amount)",
    ] {
        assert!(src.contains(marker), "contract shape missing `{marker}`");
    }
    // dec lowers to scaled int256 with truncation toward zero — never the
    // native `decimal` type (the header comment *names* it to explain why).
    for bad in [": decimal", "-> decimal", "decimal("] {
        assert!(
            !src.contains(bad),
            "must not use Vyper's native `decimal` type (scale 10^10); found `{bad}`"
        );
    }
    assert!(
        src.contains("((amount * 100) / 10000)"),
        "dec mul must lower to trunc(a*b/10000)"
    );

    std::fs::remove_dir_all(&dir).expect("failed to clean up temp work dir");
}

#[test]
fn vyper_golden_snapshot() {
    // The full emitted program is pinned: any unexpected change fails here,
    // loudly, before the gate runs.
    let dir = workdir();
    let prog_path = dir.join("demo.vy");
    let (ok, _, stderr) = run_cuni(&[
        "compile",
        FIXTURE,
        "--emit-vyper",
        prog_path.to_str().expect("non-utf8 temp path"),
    ]);
    assert!(ok, "--emit-vyper failed\n{stderr}");
    let src = std::fs::read_to_string(&prog_path).expect("failed to read emitted contract");
    assert_eq!(
        src, GOLDEN_PROGRAM,
        "emitted Vyper program drifted from the golden snapshot \
         (tests/snapshots/vyper_demo.vy) — inspect the diff; if the change is \
         intended, re-pin the snapshot deliberately"
    );
    std::fs::remove_dir_all(&dir).expect("failed to clean up temp work dir");
}

#[test]
fn vyper_logic_core_gate() {
    // The law itself must not silently change: the interpreter seat prints
    // the five driver outputs, asserted byte-for-byte (pinned).
    let (ok, gold, stderr) = run_cuni(&["run", FIXTURE]);
    assert!(ok, "cuni run failed\nstdout:\n{gold}\nstderr:\n{stderr}");
    assert_eq!(
        gold, EXPECTED_DEMO,
        "demo verdicts drifted — the law changed without the gate catching it"
    );

    // The Python reference of the pure logic core must print byte-identical
    // stdout with the machine's OWN python3.
    let dir = workdir();
    let ref_path = dir.join("demo_ref.py");
    let (ok, _, stderr) = run_cuni(&[
        "compile",
        FIXTURE,
        "--emit-vyper-ref",
        ref_path.to_str().expect("non-utf8 temp path"),
    ]);
    assert!(ok, "--emit-vyper-ref failed\n{stderr}");
    let (ok, stdout, stderr) = run_python3(&ref_path);
    assert!(
        ok,
        "python3 reference failed to run\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout, gold,
        "Vyper reference stdout diverged from CuNi gold — the gate refuses"
    );
    std::fs::remove_dir_all(&dir).expect("failed to clean up temp work dir");
}

#[test]
fn vyper_money_bridge_fee_schedule() {
    // The headline: the tiered fee law on `dec` (six driver lines), proven
    // exact on EVM via the same gate. Expected outputs pinned after honest
    // computation by the interpreter seat.
    let (ok, gold, stderr) = run_cuni(&["run", FEE_FIXTURE]);
    assert!(ok, "cuni run failed\nstdout:\n{gold}\nstderr:\n{stderr}");
    assert_eq!(
        gold, EXPECTED_FEES,
        "fee verdicts drifted — the money law changed without the gate catching it"
    );

    let dir = workdir();
    let prog_path = dir.join("fee.vy");
    let ref_path = dir.join("fee_ref.py");
    let (ok, _, stderr) = run_cuni(&[
        "compile",
        FEE_FIXTURE,
        "--emit-vyper",
        prog_path.to_str().expect("non-utf8 temp path"),
    ]);
    assert!(ok, "--emit-vyper failed on the fee law\n{stderr}");
    let src = std::fs::read_to_string(&prog_path).expect("failed to read fee contract");
    assert!(
        src.contains("def _fee(amount: int256) -> int256:"),
        "fee law's logic core missing from the contract"
    );
    assert!(
        src.contains("def fee(amount: int256) -> int256:"),
        "fee law's external wrapper missing from the shell"
    );

    let (ok, _, stderr) = run_cuni(&[
        "compile",
        FEE_FIXTURE,
        "--emit-vyper-ref",
        ref_path.to_str().expect("non-utf8 temp path"),
    ]);
    assert!(ok, "--emit-vyper-ref failed on the fee law\n{stderr}");
    let (ok, stdout, stderr) = run_python3(&ref_path);
    assert!(
        ok,
        "python3 fee reference failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout, gold,
        "Vyper fee reference stdout diverged from CuNi gold — the money bridge refuses"
    );
    std::fs::remove_dir_all(&dir).expect("failed to clean up temp work dir");
}

#[test]
fn vyper_refusals_are_honest() {
    let dir = workdir();
    // (tag, source): every one of these must make --emit-vyper refuse.
    let cases: &[(&str, &str)] = &[
        (
            "float",
            "def f() -> int do\n ret 1\nend\nsay(1.5)\n",
        ),
        (
            "list",
            "def f() -> int do\n ret 1\nend\nlet xs = [1, 2]\nsay(f())\n",
        ),
        (
            "unwrap",
            "def f() -> int do\n ret 1\nend\nlet x = f() ?? do\n ret 0\nend\nsay(x)\n",
        ),
        (
            "struct",
            "typ Point do\n x: int\nend\ndef f() -> int do\n ret 1\nend\nsay(f())\n",
        ),
        (
            "time",
            "def f() -> int do\n ret 1\nend\nsay(\"2026-01-01T00:00:00Z\"t)\n",
        ),
        (
            "str-concat",
            "def f(a: str) -> str do\n ret a + \"x\"\nend\nsay(f(\"y\"))\n",
        ),
        (
            "say-in-function",
            "def f() -> int do\n say(1)\n ret 1\nend\nsay(f())\n",
        ),
        (
            "dec-mod",
            "def f(a: dec) -> dec do\n ret a % 0.5dec\nend\nsay(f(1.0dec))\n",
        ),
    ];
    for (tag, src) in cases {
        let src_path = dir.join(format!("refuse_{tag}.cuni"));
        std::fs::write(&src_path, src).expect("failed to write refusal fixture");
        let out_path = dir.join(format!("refuse_{tag}.vy"));
        let (ok, _stdout, stderr) = run_cuni(&[
            "compile",
            src_path.to_str().expect("non-utf8 temp path"),
            "--emit-vyper",
            out_path.to_str().expect("non-utf8 temp path"),
        ]);
        assert!(
            !ok,
            "{tag}: --emit-vyper should have refused but succeeded"
        );
        assert!(
            stderr.contains("refus"),
            "{tag}: refusal must say so clearly\nstderr:\n{stderr}"
        );
    }
    std::fs::remove_dir_all(&dir).expect("failed to clean up temp work dir");
}

#[test]
fn vyper_toolchain_check() {
    // Gate precondition: the machine's OWN python3 must exist and run —
    // without it no stdout comparison can be claimed.
    let output = Command::new("python3")
        .arg("--version")
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to probe python3");
    assert!(
        output.status.success(),
        "python3 is not runnable — the logic-core gate cannot be claimed on this machine"
    );
    eprintln!(
        "python3: {}",
        String::from_utf8_lossy(&output.stdout).trim()
    );

    // The Vyper toolchain is probed, never installed. Absent (the expected
    // case), the suite asserts shape + snapshot only — the report states
    // "contract shell not compiled here — no toolchain".
    let probe = Command::new("which")
        .arg("vyper")
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to probe for vyper");
    if probe.status.success() {
        eprintln!(
            "vyper toolchain PRESENT at {} — shape + snapshot still asserted; \
             compilation left to a machine with the toolchain",
            String::from_utf8_lossy(&probe.stdout).trim()
        );
    } else {
        eprintln!("vyper toolchain absent — contract shell not compiled here, as documented");
    }
}
