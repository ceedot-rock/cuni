//! Black-box tests for `cuni check` — the platform exactness gate.

use std::path::PathBuf;
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

fn check(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(cuni_bin())
        .arg("check")
        .args(args)
        .output()
        .expect("spawn cuni");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn check_full_example_passes() {
    let (ok, stdout, stderr) = check(&["examples/full.cuni", "--timeout", "120"]);
    assert!(
        ok,
        "expected PASS\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert!(
        stdout.contains("exactness: PASS (py/go/js)"),
        "stdout missing PASS line:\n{}",
        stdout
    );
}

#[test]
fn check_structs_passes() {
    let (ok, stdout, _) = check(&["examples/structs.cuni", "--timeout", "120"]);
    assert!(ok, "{}", stdout);
    assert!(stdout.contains("exactness: PASS (py/go/js)"));
}

#[test]
fn check_named_fields_passes() {
    let (ok, stdout, _) = check(&["examples/named_fields.cuni", "--timeout", "120"]);
    assert!(ok, "{}", stdout);
    assert!(stdout.contains("exactness: PASS (py/go/js)"));
}

#[test]
fn check_modules_fails_js_refuse() {
    // modules.cuni refuses JS emit — exactness must FAIL (not silent pass)
    let (ok, stdout, _) = check(&["examples/modules.cuni", "--timeout", "60"]);
    assert!(!ok, "expected FAIL for modules.cuni, got:\n{}", stdout);
    assert!(
        stdout.contains("exactness: FAIL") || stdout.contains("REFUSE"),
        "expected refuse/fail messaging:\n{}",
        stdout
    );
}
