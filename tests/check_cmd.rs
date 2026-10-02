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

/// `examples/full.cuni` uses floats and iface — Solidity has no float type,
/// so the sol seat must YEET with a clear reason (not silently emit). The 7
/// core-subset native seats (php, pl, r, ml, lisp, pas, f90) also honestly
/// refuse: iface is beyond their core subset. The other 105 seats emit+run.
/// This pins the documented contract that honest refusal beats approximation.
#[test]
fn check_full_example_sol_honest_refusal() {
    let (ok, stdout, _) = check(&["examples/full.cuni", "--timeout", "180"]);
    assert!(!ok, "expected overall FAIL from the sol yeet:\n{}", stdout);
    assert!(
        stdout.contains("emit sol") && stdout.contains("REFUSE"),
        "expected a clean sol refusal line:\n{}",
        stdout
    );
    assert!(
        stdout.contains("no float type"),
        "refusal must name the reason:\n{}",
        stdout
    );
    assert!(
        stdout.contains("19/53 ok"),
        "all seats except honest refusers must still emit+run:\n{}",
        stdout
    );
}

/// `examples/structs.cuni` declares a custom `typ` — no Solidity mapping, so
/// sol yeets with a clear reason. The 7 core-subset seats also refuse (typ
/// beyond core subset). The other 105 seats emit+run.
#[test]
fn check_structs_sol_honest_refusal() {
    let (ok, stdout, _) = check(&["examples/structs.cuni", "--timeout", "180"]);
    assert!(!ok, "expected overall FAIL from the sol yeet:\n{}", stdout);
    assert!(
        stdout.contains("emit sol") && stdout.contains("REFUSE"),
        "expected a clean sol refusal line:\n{}",
        stdout
    );
    assert!(
        stdout.contains("no Solidity mapping"),
        "refusal must name the reason:\n{}",
        stdout
    );
    assert!(
        stdout.contains("19/53 ok"),
        "all seats except honest refusers must still emit+run:\n{}",
        stdout
    );
}

/// `examples/named_fields.cuni` — same honest-refusal contract as structs:
/// sol + 7 core-subset seats refuse, 105 others emit+run.
#[test]
fn check_named_fields_sol_honest_refusal() {
    let (ok, stdout, _) = check(&["examples/named_fields.cuni", "--timeout", "180"]);
    assert!(!ok, "expected overall FAIL from the sol yeet:\n{}", stdout);
    assert!(
        stdout.contains("emit sol") && stdout.contains("REFUSE"),
        "expected a clean sol refusal line:\n{}",
        stdout
    );
    assert!(
        stdout.contains("19/53 ok"),
        "all seats except honest refusers must still emit+run:\n{}",
        stdout
    );
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
fn run_fib_prints_55() {
    let output = Command::new(cuni_bin())
        .args(["run", "examples/compute/fib.cuni"])
        .output()
        .expect("spawn cuni run");
    assert!(
        output.status.success(),
        "cuni run failed\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "55\n");
}

#[test]
fn run_unknown_seat_refuses() {
    let output = Command::new(cuni_bin())
        .args(["run", "examples/compute/fib.cuni", "--lang", "cobol"])
        .output()
        .expect("spawn cuni run");
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("native seat") || err.contains("cobol"),
        "unexpected stderr:\n{err}"
    );
}

#[test]
fn check_compute_native_seats() {
    let (ok, stdout, stderr) = check(&[
        "examples/compute",
        "--only",
        "py,go,js,c,cpp,rs",
        "--timeout",
        "180",
    ]);
    assert!(ok, "compute native seats failed\n{stdout}\n{stderr}");
    assert!(stdout.contains("exactness: PASS"));
}

#[test]
fn bank_paste_py_to_py() {
    let output = Command::new(cuni_bin())
        .args([
            "bank",
            "paste",
            "examples/bank/add.py",
            "--from",
            "py",
            "--to",
            "py",
        ])
        .output()
        .expect("bank");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "bank paste failed\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains("bank: PASS"), "{stdout}");
    assert!(stdout.contains("source_hash="), "{stdout}");
}

#[test]
fn ingest_python_subset_roundtrip() {
    let dir = std::env::temp_dir();
    let py = dir.join(format!("cuni_ing_{}.py", std::process::id()));
    std::fs::write(
        &py,
        "def add(a, b):\n    return a + b\nprint(add(2, 40))\nprint(\"cuni\")\n",
    )
    .unwrap();
    let output = Command::new(cuni_bin())
        .args(["ingest", py.to_str().unwrap()])
        .output()
        .expect("ingest");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cuni = String::from_utf8_lossy(&output.stdout);
    assert!(cuni.contains("def add"));
    assert!(cuni.contains("say(add(2, 40))"));
}

#[test]
fn check_ext_collision_fails_js_yeet() {
    // ext-collision.cuni gets yeeted on JS emit — exactness must FAIL (not silent pass)
    let (ok, stdout, _) = check(&["examples/ext-collision.cuni", "--timeout", "60"]);
    assert!(!ok, "expected FAIL for ext-collision.cuni, got:\n{}", stdout);
    assert!(
        stdout.contains("exactness: FAIL") || stdout.contains("REFUSE"),
        "expected refuse/fail messaging:\n{}",
        stdout
    );
}
