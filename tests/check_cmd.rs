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
    let (ok, stdout, stderr) = check(&["examples/full.cuni", "--timeout", "180"]);
    assert!(
        ok,
        "expected PASS\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert!(
        stdout.contains("exactness: PASS (") && stdout.contains("langs)"),
        "stdout missing PASS line:\n{}",
        stdout
    );
}

#[test]
fn check_structs_passes() {
    let (ok, stdout, _) = check(&["examples/structs.cuni", "--timeout", "180"]);
    assert!(ok, "{}", stdout);
    assert!(stdout.contains("exactness: PASS (") && stdout.contains("langs)"));
}

#[test]
fn check_named_fields_passes() {
    let (ok, stdout, _) = check(&["examples/named_fields.cuni", "--timeout", "180"]);
    assert!(ok, "{}", stdout);
    assert!(stdout.contains("exactness: PASS (") && stdout.contains("langs)"));
}

#[test]
fn check_native_seats_full() {
    let (ok, stdout, stderr) = check(&[
        "examples/full.cuni",
        "--only",
        "py,go,js,c,cpp,rs",
        "--timeout",
        "180",
    ]);
    assert!(ok, "native seats failed\n{stdout}\n{stderr}");
    assert!(stdout.contains("exactness: PASS"));
}

#[test]
fn ingest_python_subset_roundtrip() {
    let dir = std::env::temp_dir();
    let py = dir.join(format!("cuni_ing_{}.py", std::process::id()));
    std::fs::write(&py, "def add(a, b):\n    return a + b\nprint(add(2, 40))\nprint(\"cuni\")\n").unwrap();
    let output = Command::new(cuni_bin())
        .args(["ingest", py.to_str().unwrap()])
        .output()
        .expect("ingest");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let cuni = String::from_utf8_lossy(&output.stdout);
    assert!(cuni.contains("def add"));
    assert!(cuni.contains("say(add(2, 40))"));
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
