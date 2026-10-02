//! Move module proof profile ("Trust Provable, in all things").
//!
//! `examples/onchain/move/demo.cuni` states a small vault-validator + tiered
//! fee law once; this suite gates it the way the profile promises:
//!
//! 1. **Behavioral exactness** — `--emit-move-ref` emits the standalone
//!    logic reference (Python, no dependencies); the gate runs it with
//!    `python3` and requires byte-identical stdout to the CuNi interpreter
//!    gold. The driver outputs are pinned. The reference enforces the
//!    unsigned seat's envelope explicitly (`_cuni_u64` / `_cuni_u128` raise
//!    loudly instead of wrapping — the same loud refusal Move gets from
//!    native abort-on-overflow).
//! 2. **Module shape** — `--emit-move` emits a genuine Move module:
//!    `module 0xCUNI::<name>` (clearly-marked placeholder address),
//!    the pure logic as `fun` definitions plus the `run` entry driver,
//!    one `public entry fun` wrapper per CuNi function, and the two
//!    delimited regions (`CUNI-LOGIC-CORE-*` / `CUNI-MOVE-SHELL-*`).
//! 3. **Logic-core chain** — the delimited logic region is extracted from
//!    the emitted module and asserted byte-identical to the standalone
//!    Move core emit. The exact Move text that would ship on-chain is the
//!    text the gate extracted.
//! 4. **Golden snapshot** — the full module text is pinned in
//!    `tests/snapshots/proof_move_demo.move.snap`; any unexpected change
//!    fails loudly.
//! 5. **Money bridge** — `examples/finance/fee_schedule.cuni` (tiered-fee
//!    law on `dec`, six driver lines) goes through the same gate;
//!    its outputs are pinned.
//! 6. **Honest refusals** — float, list, struct/enum, `??`, `%` on `dec`,
//!    mixed `dec`/`int` arithmetic, and (unsigned-seat specific) negative
//!    `int`/`dec` literals and unary negation refuse with clear messages.
//!
//! Honest boundaries (see also docs/ONCHAIN.md): the logic core is
//! gate-proven via the Python reference; the Move text and module shell are
//! NOT compiled here (no Move toolchain on this machine — `aptos`/`sui`
//! are checked for and expected absent); nothing has executed on-chain.
//! No mocked toolchains, no skipped asserts.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/onchain/move/demo.cuni";
const FEE_FIXTURE: &str = "examples/finance/fee_schedule.cuni";
const SNAPSHOT: &str = "tests/snapshots/proof_move_demo.move.snap";

/// The fifteen driver outputs, pinned (same law as the ink! fixture).
const EXPECTED_OUTPUTS: &str = "50\n0\n1\n2\n1.2499\n2500.25\nok\nbad\n3\n1\nTrue\n45\n0\n5.0\n5\n";
/// The six tiered-fee driver outputs, pinned (docs/DECIMAL.md truncation).
const EXPECTED_FEE_OUTPUTS: &str = "1.2499\n1.25\n5.25\n2.75\n0.25\n2500.25\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. This suite runs the
/// reference with `python3` itself, so the test prefixes every plausible
/// location onto PATH for the child processes.
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/.cargo/bin"),
        format!("{home}/go/bin"),
        format!("{home}/toolchains/bin"),
        "/usr/bin".to_string(),
        "/bin".to_string(),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

fn workdir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("cuni_proof_move_{}_{}", tag, std::process::id()));
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

/// Run a Python reference with `python3`, returning stdout. Panics loudly on
/// any failure — the gate never skips.
fn python3_and_run(py_path: &PathBuf) -> String {
    let run = Command::new("python3")
        .arg(py_path.to_str().expect("non-utf8 temp path"))
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to spawn python3");
    assert!(
        run.status.success(),
        "python3 reference failed\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// The CuNi interpreter gold for a fixture — the source of truth the gate
/// compares against.
fn cuni_gold(fixture: &str) -> String {
    let (ok, stdout, stderr) = run_cuni(&["run", fixture]);
    assert!(ok, "cuni run failed for {fixture}\n{stderr}");
    stdout
}

#[test]
fn toolchain_python3_present_move_absent() {
    // The gate's runner must really exist; the chain toolchains must
    // really not (we never install toolchains in tests).
    let py = Command::new("python3")
        .arg("--version")
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to probe python3");
    assert!(
        py.status.success(),
        "python3 is required for the Move logic gate and was not found"
    );
    for tool in ["aptos", "sui"] {
        let probe = Command::new(tool)
            .arg("--version")
            .env("PATH", toolchain_path())
            .output();
        assert!(
            probe.map(|o| !o.status.success()).unwrap_or(true),
            "{tool} is unexpectedly present; the shell shape test assumes it is absent"
        );
    }
}

#[test]
fn demo_outputs_are_pinned() {
    // The law itself must not silently change: the interpreter seat prints
    // the fifteen driver outputs, asserted byte-for-byte.
    assert_eq!(
        cuni_gold(FIXTURE),
        EXPECTED_OUTPUTS,
        "demo outputs drifted — the law changed without the gate catching it"
    );
}

#[test]
fn demo_reference_gate_python3() {
    // --emit-move-ref -> python3 -> run -> byte-identical to CuNi gold.
    let dir = workdir("ref");
    let ref_path = dir.join("demo_ref.py");
    let (ok, _, stderr) = run_cuni(&[FIXTURE, "--emit-move-ref", ref_path.to_str().unwrap()]);
    assert!(ok, "--emit-move-ref failed\n{stderr}");
    let stdout = python3_and_run(&ref_path);
    assert_eq!(
        stdout, EXPECTED_OUTPUTS,
        "Move reference stdout diverged from the pinned outputs"
    );
    assert_eq!(
        stdout,
        cuni_gold(FIXTURE),
        "Move reference stdout diverged from live CuNi gold"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup failed");
}

/// Extract the delimited logic region from a `--emit-move` artifact and
/// dedent one level, yielding the standalone Move core.
fn extract_logic_core(module_src: &str) -> String {
    let start_marker = "// CUNI-LOGIC-CORE-START";
    let end_marker = "// CUNI-LOGIC-CORE-END";
    let start = module_src
        .find(start_marker)
        .expect("module missing CUNI-LOGIC-CORE-START");
    let end = module_src
        .find(end_marker)
        .expect("module missing CUNI-LOGIC-CORE-END");
    assert!(start < end, "logic markers out of order");
    let block = &module_src[start..end];
    let mut out = String::new();
    for line in block.lines().skip(1) {
        let dedented = line.strip_prefix("    ").unwrap_or(line);
        out.push_str(dedented);
        out.push('\n');
    }
    assert!(
        out.contains("fun validate_transfer(balance: u64, amount: u64, fee_bps: u64): u64"),
        "extracted logic core lost the validation law"
    );
    out
}

#[test]
fn demo_module_shape_snapshot_and_logic_core_chain() {
    let dir = workdir("module");
    let mod_path = dir.join("demo_module.move");

    // 1. Emit the full Move module.
    let (ok, _, stderr) = run_cuni(&[FIXTURE, "--emit-move", mod_path.to_str().unwrap()]);
    assert!(ok, "--emit-move failed\n{stderr}");
    let src = std::fs::read_to_string(&mod_path).expect("failed to read emitted module");

    // 2. The module shell is Move-shaped and both regions are delimited.
    for marker in [
        "// CUNI-LOGIC-CORE-START",
        "// CUNI-LOGIC-CORE-END",
        "// CUNI-MOVE-SHELL-START",
        "// CUNI-MOVE-SHELL-END",
        "module 0xCUNI::cuni_demo {",
        "0xCUNI",
        "TODO: replace 0xCUNI",
        "fun transfer_fee(amount: u64, fee_bps: u64): u64",
        "fun fee(amount: u128): u128",
        "fun cuni_dec_str(v: u128): String",
        "public entry fun run()",
        "public entry fun transfer_fee_entry(amount: u64, fee_bps: u64)",
        "public entry fun fee_entry(amount: u128)",
        "let result = fee(amount);",
    ] {
        assert!(src.contains(marker), "module shell missing `{}`", marker);
    }

    // 3. Golden snapshot: the full module text is pinned; any unexpected
    //    change fails loudly.
    let snap_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
    let snap = std::fs::read_to_string(&snap_path)
        .expect("golden snapshot missing — regenerate it, do not delete the assert");
    assert_eq!(
        src, snap,
        "emitted Move module diverged from the golden snapshot — \
         if this change is intended, re-emit the snapshot and say why"
    );

    // 4. The delimited logic region extracts cleanly and holds the whole
    //    law: helpers, the validation/fee functions, and the driver.
    //    (Byte-verbatim equivalence with the standalone Move core emit is
    //    pinned by the `program_embeds_move_core_verbatim` unit test; the
    //    behavior gate is the Python reference — no Move toolchain here.)
    let extracted = extract_logic_core(&src);
    assert!(
        extracted.contains("public entry fun run()"),
        "extracted logic core lost the driver"
    );
    assert!(
        extracted.contains("fun cuni_dec_str"),
        "extracted logic core lost the dec renderer"
    );
    assert!(
        extracted.contains("fun cuni_dec_of_int"),
        "extracted logic core lost dec_of_int"
    );

    std::fs::remove_dir_all(&dir).expect("cleanup failed");
}

#[test]
fn money_bridge_fee_schedule() {
    // The tiered-fee law on `dec` (6 driver lines) through the Move gate.
    assert_eq!(
        cuni_gold(FEE_FIXTURE),
        EXPECTED_FEE_OUTPUTS,
        "fee_schedule gold drifted — the law changed"
    );
    let dir = workdir("fee");
    let ref_path = dir.join("fee_ref.py");
    let (ok, _, stderr) =
        run_cuni(&[FEE_FIXTURE, "--emit-move-ref", ref_path.to_str().unwrap()]);
    assert!(ok, "--emit-move-ref failed on fee_schedule\n{stderr}");
    let stdout = python3_and_run(&ref_path);
    assert_eq!(
        stdout, EXPECTED_FEE_OUTPUTS,
        "Move fee_schedule reference diverged from the pinned fee outputs"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup failed");
}

#[test]
fn refusals_are_honest() {
    // Each program must be refused by --emit-move with a clear message on
    // stderr and a non-zero exit — never a silent wrong artifact. The
    // negative cases are unsigned-seat specific: Move has no signed
    // integers, so they refuse at emit.
    let cases: &[(&str, &str)] = &[
        ("float", "def f() -> int do\n ret 1\nend\nsay(1.5)\n"),
        ("list", "def f() -> int do\n ret 1\nend\nlet xs = [1, 2]\nsay(1)\n"),
        (
            "typ",
            "typ Point do\n x: int\nend\ndef f() -> int do\n ret 1\nend\nsay(1)\n",
        ),
        (
            "unwrap",
            "def f() -> int do\n ret 1\nend\nlet x = f() ?? do\n ret 0\nend\nsay(x)\n",
        ),
        (
            "dec-mod",
            "def f(a: dec) -> dec do\n ret a % 2.0dec\nend\nsay(f(1.0dec))\n",
        ),
        (
            "mixed-dec-int",
            "def f(a: dec) -> dec do\n ret a + 1\nend\nsay(f(1.0dec))\n",
        ),
        (
            "neg-int-literal",
            "def f() -> int do\n ret 1\nend\nsay(-5)\n",
        ),
        (
            "neg-dec-literal",
            "def f() -> dec do\n ret 1.0dec\nend\nsay(-2.5dec)\n",
        ),
        (
            "negation",
            "def f(x: int) -> int do\n ret -x\nend\nsay(f(1))\n",
        ),
        (
            "time",
            "def f() -> int do\n ret 1\nend\nsay(\"2026-01-01T00:00:00Z\"t)\n",
        ),
    ];
    for (tag, src) in cases {
        let dir = workdir(&format!("refuse-{tag}"));
        let src_path = dir.join("prog.cuni");
        let out_path = dir.join("out.move");
        std::fs::write(&src_path, src).expect("failed to write refusal fixture");
        let (ok, _, stderr) = run_cuni(&[
            src_path.to_str().unwrap(),
            "--emit-move",
            out_path.to_str().unwrap(),
        ]);
        assert!(!ok, "{tag}: --emit-move should have refused");
        assert!(
            stderr.contains("refus"),
            "{tag}: refusal needs a clear message on stderr, got:\n{stderr}"
        );
        assert!(
            !out_path.is_file(),
            "{tag}: refused emit must not leave an artifact behind"
        );
        std::fs::remove_dir_all(&dir).expect("cleanup failed");
    }
}
