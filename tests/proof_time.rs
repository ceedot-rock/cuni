//! Time exactness proof profile ("Trust Provable, in all things").
//!
//! `examples/proof-time/time.cuni` states one time law once — settlement,
//! vesting, expiries, rate windows — and this suite gates it the way the
//! profile promises:
//!
//! 1. **Behavioral exactness** — `cuni check --only py,go,js,ts,c,cpp,rs,
//!    rb,lua,java,sql` emits the fixture and runs each artifact with that
//!    target's OWN toolchain, requiring byte-identical stdout and
//!    `exactness: PASS` on exit 0. `time` is an int64 unix epoch, UTC only,
//!    no timezones, no `now()` (docs/TIME.md). Inexact seats don't get to
//!    silently drift: they refuse, and refusals are tested below.
//! 2. **Pinned verdicts** — the 14 driver outputs are asserted exactly, so
//!    the time law can't silently change under the gate.
//! 3. **Deployability** — `cuni check --only sol` emits a real Solidity
//!    contract and compiles it with the real `solc` (a contract has no
//!    stdout, so successful compilation to bytecode IS the sol
//!    verification). The sol seat is uint256: non-negative times only.
//! 4. **Refusals** — malformed literals fail at the front-end; `time * int`,
//!    `time + time`, and mixed time/int arithmetic are type errors with
//!    fix-its; `parse_time` of a bad timestamp fails loudly at run time;
//!    the SQL seat folds literal `parse_time` at emit and refuses dynamic
//!    strings (no loud-refusal SQL form exists); negative times refuse on
//!    sol; Solana has no time form in v1 and refuses.
//!
//! No mocked toolchains, no skipped asserts. Everything asserted here
//! really runs.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "examples/proof-time/time.cuni";
/// The 14 driver outputs, pinned: settle / int+time / cliff /
/// days_between / duration seconds / negative days_between /
/// comparisons x3 / parse_time leap day / parse_time pre-1970 /
/// add_seconds far-future / interpolation / seconds_left fn.
const EXPECTED_VERDICTS: &str = "2026-10-03T21:30:25Z\n2026-10-01T22:30:25Z\n2026-04-01T00:00:00Z\n91\n7871374\n-91\nTrue\nTrue\nTrue\n2000-02-29T12:00:00Z\n1969-12-31T23:59:59Z\n9999-12-31T23:59:59Z\nsettles 2026-10-03T21:30:25Z\n7871374\n";

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

/// Unique scratch dir per call: tests run in parallel threads of one
/// process, so the process id alone is not unique.
fn scratch_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "cuni_proof_time_{}_{}_{:?}",
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

/// Write a scratch fixture, run `cuni check` against it, return the outcome.
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

/// Write a scratch fixture, run `cuni --emit-solana` against it (the Solana
/// seat is emit-only, not part of the check gate).
fn run_scratch_emit_solana(src: &str) -> (bool, String, String) {
    let dir = scratch_dir();
    let path = dir.join("scratch.cuni");
    std::fs::write(&path, src).expect("failed to write scratch fixture");
    let out_path = dir.join("out.rs");
    let out = run_cuni(&[
        path.to_str().expect("non-utf8 temp path"),
        "--emit-solana",
        out_path.to_str().expect("non-utf8 temp path"),
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn time_exactness_gate_all_native_seats() {
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
        "exactness gate failed for the 11 native seats\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "missing 'exactness: PASS'\n{stdout}"
    );
}

#[test]
fn time_verdicts_are_pinned() {
    // The time law itself must not silently change: the interpreter seat
    // prints the 14 driver outputs, asserted byte-for-byte.
    let (ok, stdout, stderr) = run_cuni(&["run", FIXTURE]);
    assert!(ok, "cuni run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    assert_eq!(
        stdout, EXPECTED_VERDICTS,
        "time verdicts drifted — the law changed without the gate catching it"
    );
}

#[test]
fn time_sol_compiles_to_deployable_bytecode() {
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
fn time_malformed_literals_are_refused_at_the_front_end() {
    // Strict ISO-8601 UTC only: month 13, timezone offsets, fractional
    // seconds, and missing `Z` never become a value — the parser refuses.
    for src in [
        "say(\"2026-13-01T00:00:00Z\"t)\n",
        "say(\"2026-10-01T21:30:25+02:00\"t)\n",
        "say(\"2026-10-01T21:30:25.5Z\"t)\n",
        "say(\"2026-10-01T21:30:25\"t)\n",
        "say(\"2026-02-30T00:00:00Z\"t)\n",
    ] {
        let (ok, stdout, stderr) = run_scratch(src, &["--only", "py", "--timeout", "60"]);
        let both = format!("{stdout}\n{stderr}");
        assert!(!ok, "malformed time literal was accepted: {src} — it must be refused");
        assert!(
            both.contains("refus"),
            "malformed literal refusal message is unclear for {src}\n{both}"
        );
    }
}

#[test]
fn time_bad_arithmetic_shapes_are_type_errors() {
    // The closed world (docs/TIME.md §3): `*`/`/`/`%` are not defined on
    // time, `time + time` has no meaning, and time never implicitly mixes
    // with non-int. Every shape is a front-end refusal with a fix-it.
    for (src, want) in [
        ("let t = \"2026-10-01T21:30:25Z\"t\nsay(t * 2)\n", "not defined"),
        ("let t = \"2026-10-01T21:30:25Z\"t\nsay(t + t)\n", "fix-it"),
        ("let t = \"2026-10-01T21:30:25Z\"t\nsay(t + 1.5)\n", "fix-it"),
        ("let t = \"2026-10-01T21:30:25Z\"t\nsay(2 - t)\n", "fix-it"),
    ] {
        let (ok, stdout, stderr) = run_scratch(
            src,
            &["--only", "py,go,js,ts,c,cpp,rs,rb,lua,java,sql", "--timeout", "120"],
        );
        let both = format!("{stdout}\n{stderr}");
        assert!(!ok, "bad time arithmetic shape was accepted: {src} — it must be refused");
        assert!(
            both.contains(want),
            "time arithmetic refusal lost its wording for {src}\n{both}"
        );
    }
}

#[test]
fn time_parse_time_of_bad_input_is_loud() {
    // `parse_time` of a malformed timestamp fails loudly at run time on
    // every seat — never a silent value. `cuni check` must fail at the RUN
    // stage (the program is well-typed; only the timestamp is bad).
    let (ok, stdout, stderr) = run_scratch(
        "say(parse_time(\"tomorrow\"))\n",
        &["--only", "py,go,js,rs,rb,lua,java", "--timeout", "180"],
    );
    let both = format!("{stdout}\n{stderr}");
    assert!(!ok, "parse_time of a bad timestamp was accepted — it must fail loudly");
    assert!(
        both.contains("front-end  ok"),
        "parse_time failure did not reach the run stage\n{both}"
    );
    // And the seat's own error names parse_time loudly (direct run of the
    // emitted artifact, real toolchain).
    let dir = scratch_dir();
    let src_path = dir.join("bad.cuni");
    std::fs::write(&src_path, "say(parse_time(\"tomorrow\"))\n").expect("write scratch");
    let py_path = dir.join("bad.py");
    let (emit_ok, eo, ee) = run_cuni(&[
        src_path.to_str().expect("non-utf8"),
        "--emit",
        "py",
        py_path.to_str().expect("non-utf8"),
    ]);
    assert!(emit_ok, "emit of bad parse_time program failed\n{eo}\n{ee}");
    let out = Command::new("python3")
        .arg(&py_path)
        .output()
        .expect("failed to run python3");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!out.status.success(), "bad parse_time ran clean — it must fail");
    assert!(
        err.contains("parse_time") && err.to_lowercase().contains("refus"),
        "parse_time failure message is unclear\n{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn time_sql_seat_folds_literals_and_refuses_dynamic_strings() {
    // The SQL seat folds literal `parse_time` at emit (exact, loud on bad
    // input). A dynamic string has no loud-refusal SQL form (SQLite
    // resolves names at prepare time and never errors on bad values at
    // run time), so the seat refuses the program instead of risking a
    // silent value.
    let (ok, _, _) = run_scratch(
        "say(parse_time(\"2026-10-01T21:30:25Z\"))\n",
        &["--only", "sql", "--timeout", "60"],
    );
    assert!(ok, "SQL seat refused a literal parse_time — it should fold it");

    let (ok, stdout, stderr) = run_scratch(
        "let s = \"2026-10-01T\" + \"21:30:25Z\"\nsay(parse_time(s))\n",
        &["--only", "sql", "--timeout", "60"],
    );
    let both = format!("{stdout}\n{stderr}");
    assert!(
        !ok,
        "SQL seat accepted parse_time of a dynamic string — it must refuse"
    );
    assert!(
        both.contains("refus"),
        "dynamic parse_time refusal message is unclear\n{both}"
    );
}

#[test]
fn time_sol_seat_refuses_negative_times() {
    // The sol seat is uint256: a negative epoch (pre-1970) refuses at emit.
    let (ok, stdout, stderr) = run_scratch(
        "say(\"1969-12-31T23:59:59Z\"t)\n",
        &["--only", "sol", "--timeout", "120"],
    );
    let both = format!("{stdout}\n{stderr}");
    assert!(!ok, "sol seat accepted a negative time — it must refuse");
    assert!(
        both.contains("refus"),
        "sol negative-time refusal message is unclear\n{both}"
    );
}

#[test]
fn time_solana_seat_refuses() {
    // v1 has no time form in the Solana logic core — refuse, never fake.
    let (ok, stdout, stderr) =
        run_scratch_emit_solana("say(\"2026-10-01T21:30:25Z\"t)\n");
    let both = format!("{stdout}\n{stderr}");
    assert!(!ok, "Solana seat accepted a time literal — it must refuse");
    assert!(
        both.contains("refus"),
        "Solana time refusal message is unclear\n{both}"
    );
}
