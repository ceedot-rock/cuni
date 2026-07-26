//! Tests for `src/typeck.rs`, CuNi's real type/effect checker (SPEC.md §19).
//! Unlike `tests/conformance.rs` (which proves valid programs behave
//! identically across targets), these prove the checker actually *refuses*
//! invalid ones — the other half of "compile-or-refuse" (SPEC.md §2) that
//! nothing in this codebase tested before this module existed. Each fixture
//! in `tests/typeck_invalid/` is deliberately broken in exactly one way;
//! `examples/typeck_valid_iface.cuni` is the positive control, proving the
//! `typ X is Y` conformance check doesn't false-positive on a program that
//! actually satisfies the interface.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

fn compile_error(source: &str) -> Option<String> {
    let out_path = std::env::temp_dir().join(format!("cuni_typeck_test_{}_{}.py", std::process::id(), source.replace(['/', '.'], "_")));
    let output = Command::new(cuni_bin()).arg(source).arg("--emit-py").arg(&out_path).output().expect("failed to invoke cuni binary");
    let _ = std::fs::remove_file(&out_path);
    if output.status.success() {
        None
    } else {
        Some(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn assert_rejected(source: &str, must_contain: &str) {
    let err = compile_error(source).unwrap_or_else(|| panic!("{} was expected to be rejected by the type checker, but compiled successfully", source));
    assert!(err.contains("type error"), "{} was rejected, but not by the type checker:\n{}", source, err);
    assert!(err.contains(must_contain), "{} was rejected, but the message didn't mention {:?}:\n{}", source, must_contain, err);
    // Step 2: type errors must include file:line:col (byte span → location).
    // e.g. cuni: tests/.../x.cuni:1:9: type error: ...
    let has_loc = err.lines().any(|l| {
        l.contains("type error")
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
        "{} type error missing file:line:col location:\n{}",
        source, err
    );
}

#[test]
fn undefined_variable_is_rejected() {
    assert_rejected("tests/typeck_invalid/undefined_var.cuni", "undefined variable `y`");
}

#[test]
fn push_on_let_bound_list_is_rejected() {
    assert_rejected("tests/typeck_invalid/immutable_push.cuni", "`xs`");
}

#[test]
fn assign_to_let_binding_is_rejected() {
    assert_rejected("tests/typeck_invalid/immutable_assign.cuni", "`x`");
}

#[test]
fn fail_outside_fallible_function_is_rejected() {
    assert_rejected("tests/typeck_invalid/fail_outside_fallible.cuni", "non-fallible");
}

#[test]
fn wrong_call_arg_count_is_rejected() {
    assert_rejected("tests/typeck_invalid/wrong_arg_count.cuni", "`add` expects 2 argument(s), found 3");
}

#[test]
fn unknown_type_name_is_rejected() {
    assert_rejected("tests/typeck_invalid/unknown_type.cuni", "unknown type `sttr`");
}

#[test]
fn typ_is_iface_without_matching_function_is_rejected() {
    assert_rejected("tests/typeck_invalid/iface_mismatch.cuni", "Shape.area");
}

#[test]
fn ret_type_mismatch_is_rejected() {
    assert_rejected("tests/typeck_invalid/ret_mismatch.cuni", "declares `-> int`");
}

#[test]
fn fallible_call_without_unwrap_is_rejected() {
    assert_rejected("tests/typeck_invalid/fallible_bare.cuni", "fallible");
}

#[test]
fn struct_constructor_wrong_arity_is_rejected() {
    assert_rejected("tests/typeck_invalid/bad_struct_arity.cuni", "`Point` expects 2 field argument(s)");
}

/// The positive control: `Circle is Shape` with a matching `def area(c:
/// Circle) -> float` must compile — proves `check_conformance` isn't just
/// rejecting everything.
#[test]
fn named_unknown_field_is_rejected() {
    assert_rejected(
        "tests/typeck_invalid/named_unknown_field.cuni",
        "no field `z`",
    );
}

#[test]
fn named_args_on_function_are_rejected() {
    assert_rejected(
        "tests/typeck_invalid/named_on_function.cuni",
        "named arguments are only allowed for typ constructors",
    );
}

#[test]
fn valid_iface_conformance_is_accepted() {
    assert_eq!(compile_error("examples/typeck_valid_iface.cuni"), None);
}
