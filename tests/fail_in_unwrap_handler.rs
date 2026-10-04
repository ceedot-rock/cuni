//! Regression tests for `fail` inside a `??` handler.
//!
//! Quirk (found 2026-10-04 during the compliance build): the typechecker
//! rejected `fail` directly inside a `??` handler even when lexically
//! inside a fallible function ("`fail` used at top level"), because
//! `check_expr` never carried the ambient function context into the
//! handler block. The checker now threads `fn_ctx` through, so `fail` in
//! a handler is accepted exactly when the enclosing function is fallible
//! (still refused at top level and in non-fallible functions — see
//! `tests/typeck.rs`).
//!
//! Runtime semantics (SPEC.md §12): a `fail` in the handler propagates on
//! the enclosing fallible function's failure channel, just like a `fail`
//! in the body. These tests are black-box: emit the fixture to `sql` and
//! `py`, run each artifact with that target's own toolchain (sqlite3 /
//! python3), and require byte-identical stdout.
//!
//! The fixture also covers a latent SQL-seat kind bug this change
//! unlocked: inlining a function whose every live return path was `fail`
//! left the inlined value with kind Null, so a *second* call site doing
//! arithmetic on the unwrapped value was refused ("arithmetic needs
//! numeric operands"). `inline_fn` now falls back to the declared return
//! type's kind for Null-kind returns.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cuni_failhandler_{}_{}", std::process::id(), name))
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
    let cuni = dir.join(format!("cuni_failhandler_{}_{}.cuni", std::process::id(), name));
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

/// `fail` in a `??` handler inside a fallible function: the handler's
/// `fail` propagates on the function's failure channel, and the caller's
/// own `??` handler runs. Two sequential call sites exercise the SQL
/// seat's inlined return-kind tracking (the second site used to be
/// refused with "arithmetic needs numeric operands").
#[test]
fn fail_in_unwrap_handler_propagates() {
    assert_sql_matches_py(
        "propagate",
        r#"def risky(flag: bool) -> int? do
  if flag do
    fail "flagged"
  end
  ret 42
end

def guarded(flag: bool) -> int? do
  let v = risky(flag) ?? do
    fail "guarded gave up"
  end
  ret v + 1
end

let a = guarded(false) ?? do
  say("fallback-a")
  ret 0
end
say(a)
let b = guarded(true) ?? do
  say("fallback-b")
  ret 0
end
say(b)
"#,
        "43\nfallback-b\n",
    );
}
