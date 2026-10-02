//! ink! contract proof profile ("Trust Provable, in all things").
//!
//! `examples/onchain/ink/demo.cuni` states a small vault-validator + tiered
//! fee law once; this suite gates it the way the profile promises:
//!
//! 1. **Behavioral exactness** — `--emit-ink-ref` emits the standalone logic
//!    reference (real Rust, no dependencies); the gate compiles it with
//!    plain `rustc`, runs it, and requires byte-identical stdout to the
//!    CuNi interpreter gold. The driver outputs are pinned.
//! 2. **Contract shape** — `--emit-ink` emits a genuine ink! contract:
//!    `#[ink::contract]`, `#[ink(storage)]`, `#[ink(constructor)]`,
//!    one `#[ink(message)]` per CuNi function, and the two delimited
//!    regions (`CUNI-LOGIC-CORE-*` / `CUNI-INK-SHELL-*`).
//! 3. **Logic-core chain** — the delimited `mod logic` is extracted from
//!    the emitted contract, compiled STANDALONE with plain `rustc` (the
//!    shell is excluded by construction), run, and its stdout is asserted
//!    byte-identical to the pinned outputs. The exact code that would ship
//!    inside the on-chain contract is the code the gate ran.
//! 4. **Golden snapshot** — the full contract text is pinned in
//!    `tests/snapshots/proof_ink_demo.rs.snap`; any unexpected change
//!    fails loudly.
//! 5. **Money bridge** — `examples/finance/fee_schedule.cuni` (tiered-fee
//!    law on `dec`, six driver lines) goes through the same gate;
//!    its outputs are pinned.
//! 6. **Honest refusals** — float, list, struct/enum, `??`, `%` on `dec`,
//!    and mixed `dec`/`int` arithmetic refuse with clear messages.
//!
//! Honest boundaries (see also docs/ONCHAIN.md): the logic core is
//! gate-proven; the contract shell is NOT compiled here (no ink! toolchain
//! on this machine — `cargo-contract` is checked for and expected absent);
//! nothing has executed on-chain. No mocked toolchains, no skipped asserts.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/onchain/ink/demo.cuni";
const FEE_FIXTURE: &str = "examples/finance/fee_schedule.cuni";
const SNAPSHOT: &str = "tests/snapshots/proof_ink_demo.rs.snap";

/// The fifteen driver outputs, pinned: transfer_fee / validate x3 / fee x2 /
/// label x2 / div / mod / comparison / sum_to / countdown / dec_of_int /
/// int_of_dec.
const EXPECTED_OUTPUTS: &str = "50\n0\n1\n2\n1.2499\n2500.25\nok\nbad\n3\n1\nTrue\n45\n0\n5.0\n5\n";
/// The six tiered-fee driver outputs, pinned (docs/DECIMAL.md truncation).
const EXPECTED_FEE_OUTPUTS: &str = "1.2499\n1.25\n5.25\n2.75\n0.25\n2500.25\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. This suite compiles the
/// reference and the extracted logic core with `rustc` itself, so the test
/// prefixes every plausible location onto PATH for the child processes.
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/.cargo/bin"),
        format!("{home}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"),
        format!("{home}/go/bin"),
        format!("{home}/toolchains/bin"),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

fn workdir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("cuni_proof_ink_{}_{}", tag, std::process::id()));
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

/// Compile a Rust file with plain `rustc` (no dependencies) and run it,
/// returning stdout. Panics loudly on any failure — the gate never skips.
fn rustc_and_run(rs_path: &PathBuf, bin_path: &PathBuf) -> String {
    let compile = Command::new("rustc")
        .args([
            "-O",
            "-o",
            bin_path.to_str().expect("non-utf8 temp path"),
            rs_path.to_str().expect("non-utf8 temp path"),
        ])
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to spawn rustc");
    assert!(
        compile.status.success(),
        "reference did not compile with plain rustc\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(bin_path)
        .output()
        .expect("failed to run compiled reference");
    assert!(run.status.success(), "compiled reference failed to run");
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
fn toolchain_rustc_present_cargo_contract_absent() {
    // The gate's compiler must really exist; the chain toolchain must
    // really not (we never install toolchains in tests).
    let rustc = Command::new("rustc")
        .arg("--version")
        .env("PATH", toolchain_path())
        .output()
        .expect("failed to probe rustc");
    assert!(
        rustc.status.success(),
        "rustc is required for the ink! logic gate and was not found"
    );
    let cc = Command::new("cargo-contract")
        .arg("--version")
        .env("PATH", toolchain_path())
        .output();
    assert!(
        cc.map(|o| !o.status.success()).unwrap_or(true),
        "cargo-contract is unexpectedly present; the shell shape test assumes it is absent"
    );
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
fn demo_reference_gate_rustc() {
    // --emit-ink-ref -> plain rustc -> run -> byte-identical to CuNi gold.
    let dir = workdir("ref");
    let ref_path = dir.join("demo_ref.rs");
    let bin_path = dir.join("demo_ref_bin");
    let (ok, _, stderr) = run_cuni(&[FIXTURE, "--emit-ink-ref", ref_path.to_str().unwrap()]);
    assert!(ok, "--emit-ink-ref failed\n{stderr}");
    let stdout = rustc_and_run(&ref_path, &bin_path);
    assert_eq!(
        stdout, EXPECTED_OUTPUTS,
        "ink! reference stdout diverged from the pinned outputs"
    );
    assert_eq!(
        stdout,
        cuni_gold(FIXTURE),
        "ink! reference stdout diverged from live CuNi gold"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup failed");
}

/// Extract the delimited logic module from a `--emit-ink` artifact: unwrap
/// `mod logic { ... }`, drop the single shell-owned no_std adaptation
/// import (`use ink::prelude::string::String;` — documented in the
/// emitter), dedent one level.
fn extract_logic_core(program_src: &str) -> String {
    let start_marker = "// CUNI-LOGIC-CORE-START";
    let end_marker = "// CUNI-LOGIC-CORE-END";
    let start = program_src
        .find(start_marker)
        .expect("contract missing CUNI-LOGIC-CORE-START");
    let end = program_src
        .find(end_marker)
        .expect("contract missing CUNI-LOGIC-CORE-END");
    assert!(start < end, "logic markers out of order");
    let block = &program_src[start..end];
    let mut lines = block.lines().skip(1).peekable();
    let open = lines.next().expect("empty logic region");
    assert_eq!(
        open.trim(),
        "mod logic {",
        "logic region must open with `mod logic {{`"
    );
    let mut out = String::new();
    for line in lines {
        if line == "}" {
            break;
        }
        if line.trim() == "use ink::prelude::string::String;" {
            continue;
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
fn demo_contract_shape_snapshot_and_standalone_logic_core() {
    let dir = workdir("contract");
    let prog_path = dir.join("demo_contract.rs");
    let core_path = dir.join("logic_core.rs");
    let bin_path = dir.join("logic_core_bin");

    // 1. Emit the full ink! contract.
    let (ok, _, stderr) = run_cuni(&[FIXTURE, "--emit-ink", prog_path.to_str().unwrap()]);
    assert!(ok, "--emit-ink failed\n{stderr}");
    let src = std::fs::read_to_string(&prog_path).expect("failed to read emitted contract");

    // 2. The contract shell is ink!-shaped and both regions are delimited.
    for marker in [
        "// CUNI-LOGIC-CORE-START",
        "// CUNI-LOGIC-CORE-END",
        "// CUNI-INK-SHELL-START",
        "// CUNI-INK-SHELL-END",
        "#[ink::contract]",
        "mod cuni_demo {",
        "#[ink(storage)]",
        "pub struct Contract {",
        "owner: AccountId,",
        "#[ink(constructor)]",
        "pub fn new() -> Self {",
        "#[ink(message)]",
        "pub fn transfer_fee(&self, amount: i64, fee_bps: i64) -> i64 {",
        "pub fn fee(&self, amount: i128) -> i128 {",
        "pub fn label(&self, code: i64) -> String {",
        "logic::transfer_fee(amount, fee_bps)",
        "logic::fee(amount)",
    ] {
        assert!(src.contains(marker), "contract shell missing `{}`", marker);
    }

    // 3. Golden snapshot: the full contract text is pinned; any unexpected
    //    change fails loudly.
    let snap_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
    let snap = std::fs::read_to_string(&snap_path)
        .expect("golden snapshot missing — regenerate it, do not delete the assert");
    assert_eq!(
        src, snap,
        "emitted ink! contract diverged from the golden snapshot — \
         if this change is intended, re-emit the snapshot and say why"
    );

    // 4. Extract the logic core and compile it STANDALONE with plain rustc
    //    (no ink!, no cargo-contract — the shell is excluded by
    //    construction). This is the most this machine can honestly verify.
    let core = extract_logic_core(&src);
    std::fs::write(&core_path, &core).expect("failed to write extracted logic core");
    let stdout = rustc_and_run(&core_path, &bin_path);
    assert_eq!(
        stdout, EXPECTED_OUTPUTS,
        "extracted logic core stdout diverged from the pinned outputs"
    );

    std::fs::remove_dir_all(&dir).expect("cleanup failed");
}

#[test]
fn money_bridge_fee_schedule() {
    // The tiered-fee law on `dec` (6 driver lines) through the ink! gate.
    assert_eq!(
        cuni_gold(FEE_FIXTURE),
        EXPECTED_FEE_OUTPUTS,
        "fee_schedule gold drifted — the law changed"
    );
    let dir = workdir("fee");
    let ref_path = dir.join("fee_ref.rs");
    let bin_path = dir.join("fee_ref_bin");
    let (ok, _, stderr) =
        run_cuni(&[FEE_FIXTURE, "--emit-ink-ref", ref_path.to_str().unwrap()]);
    assert!(ok, "--emit-ink-ref failed on fee_schedule\n{stderr}");
    let stdout = rustc_and_run(&ref_path, &bin_path);
    assert_eq!(
        stdout, EXPECTED_FEE_OUTPUTS,
        "ink! fee_schedule reference diverged from the pinned fee outputs"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup failed");
}

#[test]
fn refusals_are_honest() {
    // Each program must be refused by --emit-ink with a clear message on
    // stderr and a non-zero exit — never a silent wrong artifact.
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
            "time",
            "def f() -> int do\n ret 1\nend\nsay(\"2026-01-01T00:00:00Z\"t)\n",
        ),
    ];
    for (tag, src) in cases {
        let dir = workdir(&format!("refuse-{tag}"));
        let src_path = dir.join("prog.cuni");
        let out_path = dir.join("out.rs");
        std::fs::write(&src_path, src).expect("failed to write refusal fixture");
        let (ok, _, stderr) = run_cuni(&[
            src_path.to_str().unwrap(),
            "--emit-ink",
            out_path.to_str().unwrap(),
        ]);
        assert!(!ok, "{tag}: --emit-ink should have refused");
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
