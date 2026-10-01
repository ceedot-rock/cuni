//! Proof-firmware integration test: the refuse-to-promote profile, end to end.
//!
//! (a) POSITIVE: run `examples/proof-firmware/promote.sh` on
//!     `controller.cuni` in a temp work dir. Assert exit 0, assert
//!     `released/` contains both binaries, assert both binaries run and
//!     print byte-identical stdout equal to the golden decision trace.
//!
//! (b) NEGATIVE: run `promote.sh` on `refuse-me.cuni` (a front-end type
//!     error — `cuni check` genuinely refuses it). Assert non-zero exit and
//!     assert `released/` was NOT created.
//!
//! Both halves use a temp copy of the directory so `examples/` is never
//! polluted. Temp dirs are removed on Drop, even on assertion failure.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

fn proof_firmware_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/proof-firmware")
}

fn rustc_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME must be set");
    PathBuf::from(home).join(".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin")
}

/// Temp work dir, removed on drop (even if an assert! panics).
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cuni_proof_firmware_{}_{}",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create work dir");
        // Copy the profile sources into the work dir; promote.sh writes
        // released/ + .promote-build here, never into examples/.
        for f in ["controller.cuni", "refuse-me.cuni", "promote.sh"] {
            std::fs::copy(proof_firmware_dir().join(f), path.join(f))
                .unwrap_or_else(|e| panic!("copy {}: {}", f, e));
        }
        let sh = path.join("promote.sh");
        let mut perms = std::fs::metadata(&sh).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&sh, perms).unwrap();
        WorkDir { path }
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn promote(work: &WorkDir, source: &str) -> std::process::Output {
    let home = std::env::var("HOME").expect("HOME must be set");
    let path_env = format!(
        "{}:{}:/usr/bin:/bin",
        rustc_dir().display(),
        Path::new(&home).join(".cargo/bin").display()
    );
    // Invoke via `bash` rather than exec'ing the script directly: a tight
    // copy -> chmod -> execve of a script in the same thread can hit a
    // transient kernel ETXTBSY ("Text file busy") on tmpfs, because the
    // close() inside std::fs::copy defers its write-access release. Going
    // through bash execs /usr/bin/bash (never written) and only opens the
    // script read-only. The script still runs for real — no mocked gate.
    Command::new("bash")
        .arg(work.path.join("promote.sh"))
        .arg(work.path.join(source))
        .env("PROMOTE_DIR", &work.path)
        .env("CUNI_BIN", cuni_bin())
        .env("PATH", path_env)
        .output()
        .expect("failed to execute promote.sh")
}

fn run_binary(bin: &Path) -> String {
    let out = Command::new(bin)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {}", bin.display(), e));
    assert!(
        out.status.success(),
        "{} exited non-zero:\nstdout: {}\nstderr: {}",
        bin.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The exact decision trace the hysteresis controller must print on the fixed
/// input trace: ON at/below 197, OFF at/above 205, hold in the band, fault at
/// -1 holds the last decision. Derived from the source by hand and verified
/// against real runs of both compiled binaries.
const GOLDEN_TRACE: &str = "t=210 HEAT_OFF\n\
t=205 HEAT_OFF\n\
t=201 HEAT_OFF\n\
t=198 HEAT_OFF\n\
t=196 HEAT_ON\n\
t=195 HEAT_ON\n\
t=197 HEAT_ON\n\
t=200 HEAT_ON\n\
t=203 HEAT_ON\n\
t=206 HEAT_OFF\n\
t=208 HEAT_OFF\n\
t=206 HEAT_OFF\n\
t=203 HEAT_OFF\n\
t=200 HEAT_OFF\n\
t=197 HEAT_ON\n\
t=194 HEAT_ON\n\
t=192 HEAT_ON\n\
t=190 HEAT_ON\n\
t=-1 FAULT_SENSOR\n\
t=195 HEAT_ON\n\
t=200 HEAT_ON\n\
t=205 HEAT_OFF\n\
t=208 HEAT_OFF\n";

#[test]
fn promote_releases_identical_binaries_on_gate_pass() {
    let work = WorkDir::new("positive");
    let out = promote(&work, "controller.cuni");
    assert!(
        out.status.success(),
        "promote.sh exited {} on controller.cuni:\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let released = work.path.join("released");
    let bin_c = released.join("controller-c");
    let bin_rs = released.join("controller-rs");
    assert!(bin_c.is_file(), "released/ missing controller-c");
    assert!(bin_rs.is_file(), "released/ missing controller-rs");

    let stdout_c = run_binary(&bin_c);
    let stdout_rs = run_binary(&bin_rs);
    assert_eq!(
        stdout_c, stdout_rs,
        "released C and Rust binaries diverged on stdout"
    );
    assert_eq!(
        stdout_c, GOLDEN_TRACE,
        "released binaries don't match the golden decision trace"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PROMOTED"),
        "promote.sh should announce PROMOTED on success"
    );
}

#[test]
fn promote_refuses_and_releases_nothing_on_gate_fail() {
    let work = WorkDir::new("negative");
    let out = promote(&work, "refuse-me.cuni");
    assert!(
        !out.status.success(),
        "promote.sh must exit non-zero on refuse-me.cuni, got {}\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("REFUSED"),
        "refusal output should contain REFUSED, got:\n{}",
        combined
    );
    assert!(
        combined.contains("binary NOT promoted"),
        "refusal output should say the binary was NOT promoted, got:\n{}",
        combined
    );
    assert!(
        !work.path.join("released").exists(),
        "released/ must NOT be created when the gate fails"
    );
}
