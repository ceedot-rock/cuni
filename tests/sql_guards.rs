//! Regression tests for the SQL seat's `alive` threading after `if`.
//!
//! Bug (found 2026-10-02 during the compliance build, fixed for 0.9.0):
//! `exec_if` restored `alive` to the pre-`if` value after the branches,
//! ignoring the `returned` accumulator that `do_return` maintains. A
//! `ret`/`fail` inside any `if` branch therefore never suppressed the
//! code after the `if` — guarded `say`s fired on both the taken branch
//! and the fallthrough path, while every other seat ran only the branch.
//!
//! These tests are black-box: emit the fixture to `sql` and `py`, run
//! each artifact with that target's own toolchain (sqlite3 / python3),
//! and require byte-identical stdout.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cuni_sqlguards_{}_{}", std::process::id(), name))
}

fn emit(source: &PathBuf, seat: &str, out: &PathBuf) {
    let output = Command::new(cuni_bin())
        .arg(source)
        .arg("--emit")
        .arg(seat)
        .arg(out)
        .output()
        .expect("failed to invoke cuni binary");
    assert!(
        output.status.success(),
        "emit {seat} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(cmd: &str, args: &[&str]) -> String {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {cmd}: {e}"));
    assert!(
        output.status.success(),
        "{cmd} {args:?} exited non-zero:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Emit `src` to the sql and py seats, run both with their real
/// toolchains, and require byte-identical stdout equal to `expected`.
fn assert_sql_matches_py(name: &str, src: &str, expected: &str) {
    let dir = std::env::temp_dir();
    let cuni = dir.join(format!("cuni_sqlguards_{}_{}.cuni", std::process::id(), name));
    std::fs::write(&cuni, src).expect("write fixture");
    let sql = tmp_path(&format!("{name}.sql"));
    let py = tmp_path(&format!("{name}.py"));
    emit(&cuni, "sql", &sql);
    emit(&cuni, "py", &py);
    let out_sql = run("sqlite3", &[":memory:", &format!(".read {}", sql.display())]);
    let out_py = run("python3", &[py.to_str().unwrap()]);
    assert_eq!(out_sql, out_py, "sql and py seats diverged for {name}");
    assert_eq!(out_sql, expected, "wrong output for {name}");
    let _ = std::fs::remove_file(&cuni);
    let _ = std::fs::remove_file(&sql);
    let _ = std::fs::remove_file(&py);
}

/// The reported case: `??` upstream, three levels of nested `if`/`els`
/// with `ret` in the branches, and a `say` after the nest. Before the
/// fix the sql seat printed "five\nfell through\n"; every other seat
/// prints just "five\n".
#[test]
fn nested_if_els_after_unwrap_with_returns() {
    assert_sql_matches_py(
        "nested3",
        r#"def maybe(x: int) -> opt<int> do
  if x > 0 do
    ret(x)
  end
  ret(none)
end

def check(v: int) -> int do
  let got: int = maybe(v) ?? do ret(-99999) end
  if got == 1 do
    say("one")
    ret(1)
  els
    if got == 2 do
      say("two")
      ret(2)
    els
      if got == 5 do
        say("five")
        ret(5)
      els
        say("other")
        ret(0)
      end
    end
  end
  say("fell through")
end

for x in [5] do
  let r = check(x)
end
"#,
        "five\n",
    );
}

/// Same root cause, minimal shape: a single `if` with a `ret` in the
/// branch must still suppress the `say` after it.
#[test]
fn return_inside_if_suppresses_following_say() {
    assert_sql_matches_py(
        "single",
        r#"def f(v: int) -> int do
  if v == 1 do
    say("one")
    ret(1)
  end
  say("after")
  ret(0)
end

for x in [1] do
  let r = f(x)
end
"#,
        "one\n",
    );
}

/// A `fail` inside an `if` branch in a fallible function aborts the
/// rest of the function body on the sql seat too (previously it did
/// not — the same missing `returned` fold).
#[test]
fn fail_inside_if_aborts_fallible_function() {
    assert_sql_matches_py(
        "fail",
        r#"def maybe(x: int) -> opt<int> do
  if x > 0 do
    ret(x)
  end
  ret(none)
end

def check(v: int) -> int ? do
  let got: int = maybe(v) ?? do ret(-99999) end
  if got == 5 do
    say("five")
    fail("boom")
  end
  say("unreached")
  ret(got)
end

for x in [5] do
  let r: int = check(x) ?? do ret(-1) end
end
"#,
        "five\n",
    );
}
