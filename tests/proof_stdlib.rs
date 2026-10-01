//! Wave-1 stdlib exactness proof profile ("Trust Provable, in all things").
//!
//! `examples/stdlib-wave1/*.cuni` state the wave-1 stdlib laws once
//! (docs/STDLIB.md); this suite gates them the way the profile promises:
//!
//! 1. **Behavioral exactness** — `cuni check --only <green seats>` emits
//!    each fixture and runs every artifact with that target's OWN toolchain,
//!    requiring byte-identical stdout and `exactness: PASS` on exit 0.
//! 2. **Pinned semantics** — the interpreter's stdout for each fixture is
//!    asserted byte-for-byte, so the law can't silently change under the gate.
//! 3. **Honest refusals** — seats that cannot host a function refuse at emit
//!    time with a documented reason (sol: all wave-1; sql: json, split,
//!    join, time.parts, sha256).
//! 4. **Runtime refusals** — invalid inputs (bad JSON, float JSON, empty
//!    separator, bad date) fail on every green seat.
//!
//! No mocked toolchains, no skipped asserts. Everything asserted here really runs.

use std::path::PathBuf;
use std::process::Command;

const DIR: &str = "examples/stdlib-wave1";
/// Seats green for every wave-1 function.
const FULL: &str = "py,go,js,ts,c,cpp,rs,rb,lua,java";
/// Seats green for time.epoch / trim / contains (sql joins the full set).
const FULL_SQL: &str = "py,go,js,ts,c,cpp,rs,rb,lua,java,sql";

const EXPECT_JSON: &str = "{\"a\":1,\"b\":[1,2,{\"x\":true}],\"n\":null,\"s\":\"hi\\nthere\"}\n{\"a\":{\"q\":[3]},\"z\":1}\n{\"e\":1000,\"f\":1,\"g\":1,\"h\":0}\n{\"lt\":\"<>\",\"u\":\"\u{e9}\u{2028}\"}\n{\"dup\":2}\n";
const EXPECT_TIME_EPOCH: &str = "0\n946684800\n1709208000\n-62135596800\n253402300799\n";
const EXPECT_TIME_PARTS: &str = "{\"day\":1,\"hour\":0,\"min\":0,\"month\":1,\"sec\":0,\"year\":1970}\n{\"day\":1,\"hour\":0,\"min\":0,\"month\":1,\"sec\":0,\"year\":2000}\n{\"day\":1,\"hour\":0,\"min\":0,\"month\":1,\"sec\":0,\"year\":1}\n{\"day\":31,\"hour\":23,\"min\":59,\"month\":12,\"sec\":59,\"year\":9999}\n{\"day\":29,\"hour\":12,\"min\":0,\"month\":2,\"sec\":0,\"year\":2024}\n";
const EXPECT_STR_SPLIT: &str = "a|b|c\n3\n|a|\n\nabc\na|b|c\n";
const EXPECT_STR_JOIN: &str = "a, b, c\nx\n1 + 2\n";
const EXPECT_STR_TRIM: &str = "True\nTrue\nTrue\nTrue\nTrue\nTrue\n";
const EXPECT_STR_CONTAINS: &str = "True\nFalse\nTrue\nTrue\nFalse\n";
const EXPECT_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\nb94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. `cuni check` spawns
/// `go`, `rustc`, and `solc` directly, so the test prefixes them onto PATH
/// for the child process (grandchildren inherit it).
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/go/bin"),
        format!("{home}/toolchains/go/bin"),
        format!("{home}/toolchains/bin"),
        format!("{home}/.cargo/bin"),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
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

fn fixture(name: &str) -> String {
    format!("{DIR}/{name}.cuni")
}

/// Behavioral exactness gate for one fixture across its green seats.
fn assert_exactness(name: &str, seats: &str) {
    let (ok, stdout, stderr) = run_cuni(&[
        "check",
        &fixture(name),
        "--only",
        seats,
        "--timeout",
        "240",
    ]);
    assert!(
        ok,
        "exactness gate failed for {name} on [{seats}]\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "missing 'exactness: PASS' for {name}\n{stdout}"
    );
}

/// Pinned interpreter semantics for one fixture.
fn assert_pinned(name: &str, expected: &str) {
    let (ok, stdout, stderr) = run_cuni(&["run", &fixture(name)]);
    assert!(
        ok,
        "cuni run failed for {name}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout, expected,
        "pinned stdout drifted for {name} — the law changed without the gate catching it"
    );
}

/// A fixture must FAIL (nonzero exit) on the given seats.
fn assert_fails(name: &str, seats: &str) {
    let (ok, stdout, stderr) = run_cuni(&[
        "check",
        &fixture(name),
        "--only",
        seats,
        "--timeout",
        "240",
    ]);
    assert!(
        !ok,
        "expected failure for {name} on [{seats}] but it passed\nstdout:\n{stdout}"
    );
    let _ = stderr;
}

#[test]
fn stdlib_json_exactness() {
    assert_exactness("json", FULL);
}

#[test]
fn stdlib_time_exactness() {
    // time.epoch is also green on sql (pure integer SQL).
    assert_exactness("time-epoch", FULL_SQL);
    assert_exactness("time-parts", FULL);
}

#[test]
fn stdlib_strings_exactness() {
    assert_exactness("str-split", FULL);
    assert_exactness("str-join", FULL);
    // trim / contains are also green on sql.
    assert_exactness("str-trim", FULL_SQL);
    assert_exactness("str-contains", FULL_SQL);
}

#[test]
fn stdlib_sha_exactness() {
    assert_exactness("sha", FULL);
}

#[test]
fn stdlib_pinned_stdout() {
    assert_pinned("json", EXPECT_JSON);
    assert_pinned("time-epoch", EXPECT_TIME_EPOCH);
    assert_pinned("time-parts", EXPECT_TIME_PARTS);
    assert_pinned("str-split", EXPECT_STR_SPLIT);
    assert_pinned("str-join", EXPECT_STR_JOIN);
    assert_pinned("str-trim", EXPECT_STR_TRIM);
    assert_pinned("str-contains", EXPECT_STR_CONTAINS);
    assert_pinned("sha", EXPECT_SHA);
}

#[test]
fn stdlib_sol_refuses_all() {
    // The sol seat refuses every wave-1 function (docs/STDLIB.md §5).
    // `cuni check` treats an emit refusal as failure — that IS the assertion.
    for name in ["json", "time-epoch", "str-split", "sha"] {
        let (ok, stdout, _) = run_cuni(&["check", &fixture(name), "--only", "sol", "--timeout", "120"]);
        assert!(!ok, "sol should refuse {name} but passed\n{stdout}");
        assert!(
            stdout.contains("REFUSE") || stdout.contains("refusing"),
            "sol refusal for {name} has no documented reason\n{stdout}"
        );
    }
}

#[test]
fn stdlib_sql_refuses_non_green() {
    // sql is green only for time.epoch, trim, contains (docs/STDLIB.md §5).
    for name in ["json", "time-parts", "str-split", "str-join", "sha"] {
        let (ok, stdout, _) = run_cuni(&["check", &fixture(name), "--only", "sql", "--timeout", "120"]);
        assert!(!ok, "sql should refuse {name} but passed\n{stdout}");
        assert!(
            stdout.contains("refusing"),
            "sql refusal for {name} has no documented reason\n{stdout}"
        );
    }
}

#[test]
fn stdlib_bad_inputs_fail_everywhere() {
    // Runtime refusals: invalid input fails on every green seat.
    assert_fails("bad-json", FULL);
    assert_fails("float-json", FULL);
    assert_fails("empty-sep", FULL);
    assert_fails("bad-date", FULL_SQL);
}
