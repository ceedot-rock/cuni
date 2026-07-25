//! Conformance suite: the operational definition of "exact" for CuNi (SPEC.md
//! §2, §18). Each case compiles a `.cuni` example to Python, Go, and JS via
//! the built `cuni` binary, actually runs each output with that target's own
//! toolchain, and asserts all three produce byte-identical stdout — not just
//! that the generated code superficially looks right per target. This is
//! deliberately black-box (shells out to `python3`/`node`/`go run`) rather
//! than unit-testing codegen internals, since exactness is a claim about
//! runtime behavior, not source text.

use std::path::{Path, PathBuf};
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cuni_conformance_{}_{}", std::process::id(), name))
}

/// Runs `cuni <source> --emit-<target> <out>`, returning the emitted file's
/// path on success or the compiler's stderr on failure.
fn emit(source: &str, target: &str, out: &Path) -> Result<(), String> {
    let output = Command::new(cuni_bin())
        .arg(source)
        .arg(format!("--emit-{}", target))
        .arg(out)
        .output()
        .expect("failed to invoke cuni binary");
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn run(cmd: &str, args: &[&str]) -> String {
    let output = Command::new(cmd).args(args).output().unwrap_or_else(|e| panic!("failed to run {}: {}", cmd, e));
    assert!(
        output.status.success(),
        "{} {:?} exited non-zero:\nstdout: {}\nstderr: {}",
        cmd,
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Compiles `source` to all three v1 targets, runs each, and asserts they
/// produce identical stdout equal to `expected`.
fn assert_exact(source: &str, expected: &str) {
    let py = tmp_path(&format!("{}.py", sanitize(source)));
    let go = tmp_path(&format!("{}.go", sanitize(source)));
    let js = tmp_path(&format!("{}.js", sanitize(source)));

    emit(source, "py", &py).unwrap_or_else(|e| panic!("py emit failed for {}: {}", source, e));
    emit(source, "go", &go).unwrap_or_else(|e| panic!("go emit failed for {}: {}", source, e));
    emit(source, "js", &js).unwrap_or_else(|e| panic!("js emit failed for {}: {}", source, e));

    let out_py = run("python3", &[py.to_str().unwrap()]);
    let out_go = run("go", &["run", go.to_str().unwrap()]);
    let out_js = run("node", &[js.to_str().unwrap()]);

    assert_eq!(out_py, out_go, "python and go diverged for {}", source);
    assert_eq!(out_py, out_js, "python and js diverged for {}", source);
    assert_eq!(out_py, expected, "output for {} didn't match expected", source);

    let _ = std::fs::remove_file(&py);
    let _ = std::fs::remove_file(&go);
    let _ = std::fs::remove_file(&js);
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
}

#[test]
fn full_example_is_exact_across_targets() {
    // parse("42") succeeds → 42; Circle(2.0).area → ~12.56636; then names.push + say
    assert_exact(
        "examples/full.cuni",
        "12.56636\nfound at 3, n is 42\n",
    );
}

#[test]
fn enums_example_is_exact_across_targets() {
    // `ret 0` inside the top-level `??` handler exits implicit main, so
    // the following `say(n)` never runs — intentional §7 semantics.
    assert_exact("examples/enums.cuni", "4\ngo\nparse failed as expected\n");
}

#[test]
fn structs_example_is_exact_across_targets() {
    assert_exact("examples/structs.cuni", "7\n3\n");
}

/// `examples/modules.cuni` deliberately names its `ext` binding `fetch` while
/// its `js:` body also calls the global `fetch` — the compiler must refuse to
/// compile for JS (src/checks.rs) rather than emit self-recursive JS. This is
/// the one example that isn't a same-output-everywhere case: Python/Go still
/// compile (their own gaps — undefined `requests`/`httpGet` — are a separate,
/// unrelated, already-documented limitation of `ext` bodies assuming
/// dependencies the toy backends never resolve).
#[test]
fn modules_example_refuses_js_due_to_ext_collision() {
    let js = tmp_path("modules_refusal.js");
    let err = emit("examples/modules.cuni", "js", &js).expect_err("expected --emit-js to refuse to compile");
    assert!(err.contains("ext fetch"), "unexpected error message: {}", err);
    assert!(err.contains("shadows"), "unexpected error message: {}", err);
    assert!(!js.exists(), "refused compilation should not write an output file");

    // py/go are unaffected by the collision check (no reserved-name match for
    // either), so they should still emit successfully.
    let py = tmp_path("modules_ok.py");
    let go = tmp_path("modules_ok.go");
    emit("examples/modules.cuni", "py", &py).expect("py emit should still succeed");
    emit("examples/modules.cuni", "go", &go).expect("go emit should still succeed");
    let _ = std::fs::remove_file(&py);
    let _ = std::fs::remove_file(&go);
}

/// This is the actual point of `link` (SPEC.md §19): not that the same
/// program's Python/Go/JS outputs behave alike, but that a program compiled
/// to ONE target can be called at runtime, over a real socket, by a program
/// compiled to a DIFFERENT target — proving cross-language interop, not just
/// cross-language self-consistency. Compiles `examples/link.cuni` to Go
/// (server) and Python (client), starts the Go binary as a real child
/// process listening on a test port, and asserts a Python-compiled client
/// gets the correct answer back over HTTP+JSON. "Mount the handler" is
/// deliberately the caller's job (INTEROP_PROPOSAL.md item 6, "codegen-only,
/// no bundled runtime"), so this test hand-writes that few-line driver on
/// both ends, exactly as a real CuNi user would.
#[test]
fn link_interop_go_server_python_client() {
    let go_src = tmp_path("link_server.go");
    let py_src = tmp_path("link_client.py");
    emit("examples/link.cuni", "go", &go_src).expect("go emit should succeed for link.cuni");
    emit("examples/link.cuni", "py", &py_src).expect("py emit should succeed for link.cuni");

    // The generated `func main() {}` is always exactly this empty shape when
    // the source has no top-level statements (true for link.cuni) — replace
    // it with a hand-written driver that starts a real HTTP server, the same
    // way a real CuNi user would wire up the generated handler themselves.
    let go_code = std::fs::read_to_string(&go_src).unwrap();
    let go_code = go_code.replace(
        "func main() {\n}\n",
        "func main() {\n\thttp.HandleFunc(\"/Greet\", Greet_handler)\n\thttp.ListenAndServe(\"127.0.0.1:8947\", nil)\n}\n",
    );
    assert!(go_code.contains("127.0.0.1:8947"), "replacement of the generated empty main() didn't match — codegen output shape changed");
    std::fs::write(&go_src, go_code).unwrap();

    let py_dir = py_src.parent().unwrap();
    let py_client = py_dir.join("link_client_driver.py");
    std::fs::write(
        &py_client,
        format!(
            "import sys\nsys.path.insert(0, {:?})\nfrom {} import Greet_remote\nprint(Greet_remote(\"http://127.0.0.1:8947\", \"Cee\", 3))\n",
            py_dir.to_str().unwrap(),
            py_src.file_stem().unwrap().to_str().unwrap()
        ),
    )
    .unwrap();

    let mut server = Command::new("go").args(["run", go_src.to_str().unwrap()]).spawn().expect("failed to start go server");
    // No readiness signal from the server itself (it's a hand-written driver,
    // not part of the generated contract), and `go run` compiles from
    // scratch on a cold cache — poll for the port instead of guessing a fixed
    // delay.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if std::net::TcpStream::connect("127.0.0.1:8947").is_ok() {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = server.kill();
            panic!("go server never started listening on :8947 within 15s");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let result = std::panic::catch_unwind(|| run("python3", &[py_client.to_str().unwrap()]));
    let _ = server.kill();
    let _ = server.wait();

    let out = result.unwrap_or_else(|e| std::panic::resume_unwind(e));
    assert_eq!(out, "hello Cee x3\n");

    let _ = std::fs::remove_file(&go_src);
    let _ = std::fs::remove_file(&py_src);
    let _ = std::fs::remove_file(&py_client);
}
