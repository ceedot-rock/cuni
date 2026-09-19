//! M1 parser fidelity — oddity-matrix hard-fail fixtures.
//!
//! Proves labeled refuse diagnostics (category + fix-it + file:line:col) for
//! matrix rows that previously fell through as generic lex/parse errors or
//! silent `#` trivia. Measured gaps only — not a completeness claim.
//! Honesty: 119 catalog / ~7 native / majority lowering; IR not done.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

fn compile_stderr(source: &str) -> String {
    let out_path = std::env::temp_dir().join(format!(
        "cuni_oddity_test_{}_{}.py",
        std::process::id(),
        source.replace(['/', '.'], "_")
    ));
    let output = Command::new(cuni_bin())
        .arg(source)
        .arg("--emit-py")
        .arg(&out_path)
        .output()
        .expect("failed to invoke cuni binary");
    let _ = std::fs::remove_file(&out_path);
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_oddity_hardfail(source: &str, category: &str) {
    let err = compile_stderr(source);
    assert!(
        !err.is_empty(),
        "{} was expected to hard-fail, but produced no stderr",
        source
    );
    let tag = format!("oddity hard-fail [{}]", category);
    assert!(
        err.contains(&tag),
        "{} missing labeled refuse {:?}:\n{}",
        source,
        tag,
        err
    );
    assert!(
        err.contains("fix-it:"),
        "{} missing fix-it:\n{}",
        source,
        err
    );
    assert!(
        err.contains("no approximate mode"),
        "{} missing exactness refuse cue:\n{}",
        source,
        err
    );
    // Span → file:line:col on lex or parse error line.
    let has_loc = err.lines().any(|l| {
        (l.contains("lex error") || l.contains("parse error"))
            && l.contains(".cuni:")
            && l.split(".cuni:")
                .nth(1)
                .map(|rest| {
                    let mut it = rest.split(':');
                    let line = it.next().and_then(|s| s.parse::<u32>().ok());
                    let col = it.next().and_then(|s| s.parse::<u32>().ok());
                    matches!((line, col), (Some(l), Some(c)) if l >= 1 && c >= 1)
                })
                .unwrap_or(false)
    });
    assert!(
        has_loc,
        "{} oddity refuse missing file:line:col:\n{}",
        source,
        err
    );
}

#[test]
fn pointers_ampersand_hardfails_with_label() {
    assert_oddity_hardfail("tests/oddity_hardfail/pointers_ampersand.cuni", "pointers");
}

#[test]
fn pointers_deref_hardfails_with_label() {
    assert_oddity_hardfail("tests/oddity_hardfail/pointers_deref.cuni", "pointers");
}

#[test]
fn pointers_star_type_hardfails_with_label() {
    assert_oddity_hardfail("tests/oddity_hardfail/pointers_star_type.cuni", "pointers");
}

#[test]
fn macros_define_hardfails_with_label() {
    assert_oddity_hardfail("tests/oddity_hardfail/macros_define.cuni", "macros");
}

#[test]
fn async_await_hardfails_with_label() {
    assert_oddity_hardfail("tests/oddity_hardfail/async_await.cuni", "async");
}

#[test]
fn ownership_lifetime_hardfails_with_label() {
    assert_oddity_hardfail("tests/oddity_hardfail/ownership_lifetime.cuni", "ownership");
}

#[test]
fn prototypes_proto_hardfails_with_label() {
    assert_oddity_hardfail("tests/oddity_hardfail/prototypes_proto.cuni", "prototypes");
}

#[test]
fn ordinary_hash_comment_still_compiles() {
    let out_path = std::env::temp_dir().join(format!(
        "cuni_oddity_ok_{}.py",
        std::process::id()
    ));
    let output = Command::new(cuni_bin())
        .arg("tests/oddity_hardfail/ok_hash_comment.cuni")
        .arg("--emit-py")
        .arg(&out_path)
        .output()
        .expect("invoke cuni");
    let _ = std::fs::remove_file(&out_path);
    assert!(
        output.status.success(),
        "ordinary `# comment` must remain trivia, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
