//! Financial Division proof profile ("Trust Provable, in all things").
//!
//! `examples/finance/` states three money laws once each — tiered fees,
//! interest accrual (`dec` x `time`), marginal withholding — and this suite
//! gates them the way the division promises:
//!
//! 1. **Behavioral exactness** — `cuni check --only py,rs,go,java,sql` emits
//!    each fixture and runs every artifact with that target's OWN toolchain,
//!    requiring byte-identical stdout and `exactness: PASS`. (The `tsql`
//!    seat shares the `.sql` extension filter, so it runs too — six seats
//!    total, all byte-identical.)
//! 2. **Pinned verdicts** — every driver output is asserted exactly, so a
//!    money law can't silently change under the gate.
//! 3. **`cuni audit`** — the core product: key generation, a PASS receipt
//!    against hand-written foreign impls (py, java, rs, sql runners all
//!    exercised), signature re-verification via `cuni::audit::verify_receipt`,
//!    then a one-byte tamper → REFUSE, and a failed gold gate → REFUSE.
//!
//! No mocked toolchains, no skipped asserts. Everything asserted here
//! really runs. Throwaway keys live in temp dirs only — never in the repo.

use cuni::audit::{sha256_hex, verify_receipt};
use std::path::{Path, PathBuf};
use std::process::Command;

const FEE: &str = "examples/finance/fee_schedule.cuni";
const INTEREST: &str = "examples/finance/interest_accrual.cuni";
const WITHHOLD: &str = "examples/finance/withholding.cuni";

/// Pinned driver outputs — the money laws, byte for byte.
const FEE_VERDICTS: &str = "1.2499\n1.25\n5.25\n2.75\n0.25\n2500.25\n";
const INTEREST_VERDICTS: &str = "500.0\n249.315\n2\n2.7397\n";
const WITHHOLD_VERDICTS: &str = "0.0\n0.0\n0.001\n400.0\n400.002\n600.0\n";

/// A hand-written "bank Python" implementation of the fee law: the foreign
/// impl an auditor would check. Must print exactly FEE_VERDICTS.
const FEE_PY_IMPL: &str = "print(\"1.2499\")\nprint(\"1.25\")\nprint(\"5.25\")\nprint(\"2.75\")\nprint(\"0.25\")\nprint(\"2500.25\")\n";

/// A hand-written "bank SQL" implementation of the fee law.
const FEE_SQL_IMPL: &str = "SELECT '1.2499';\nSELECT '1.25';\nSELECT '5.25';\nSELECT '2.75';\nSELECT '0.25';\nSELECT '2500.25';\n";

/// A hand-written "bank Java" implementation of the withholding law.
const WITHHOLD_JAVA_IMPL: &str = "class Main {\n    public static void main(String[] args) {\n        System.out.println(\"0.0\");\n        System.out.println(\"0.0\");\n        System.out.println(\"0.001\");\n        System.out.println(\"400.0\");\n        System.out.println(\"400.002\");\n        System.out.println(\"600.0\");\n    }\n}\n";

/// A hand-written "auditor Rust" implementation of the interest law.
const INTEREST_RS_IMPL: &str =
    "fn main() {\n    println!(\"500.0\");\n    println!(\"249.315\");\n    println!(\"2\");\n    println!(\"2.7397\");\n}\n";

fn cuni_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cuni"))
}

/// Toolchain dirs that are NOT on the default PATH. `cuni check` / `cuni
/// audit` spawn `rustc`, `go`, `javac` directly, so the test prefixes them
/// onto PATH for the child process (grandchildren inherit it).
fn toolchain_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set to locate toolchains");
    let extra = [
        format!("{home}/go/bin"),
        format!("{home}/toolchains/go/bin"),
        format!("{home}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"),
        format!("{home}/toolchains/bin"),
    ];
    let cur = std::env::var("PATH").unwrap_or_default();
    format!("{}:{cur}", extra.join(":"))
}

/// Unique scratch dir per call: tests run in parallel threads of one
/// process, so the process id alone is not unique.
fn scratch_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "cuni_proof_finance_{tag}_{}_{}_{:?}",
        std::process::id(),
        n,
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp work dir");
    dir
}

fn run_cuni(args: &[&str], cwd: Option<&Path>) -> (bool, String, String) {
    let mut cmd = Command::new(cuni_bin());
    cmd.args(args).env("PATH", toolchain_path());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let output = cmd.output().expect("failed to spawn cuni binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_iso8601_utc(s: &str) -> bool {
    // YYYY-MM-DDTHH:MM:SSZ — exactly 20 chars.
    s.len() == 20
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[7] == b'-'
        && s.as_bytes()[10] == b'T'
        && s.as_bytes()[13] == b':'
        && s.as_bytes()[16] == b':'
        && s.as_bytes()[19] == b'Z'
        && s[..4].chars().all(|c| c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// 1. Behavioral exactness: every money seat, byte-identical stdout.
// ---------------------------------------------------------------------------

fn gate_fixture(fixture: &str) {
    let (ok, stdout, stderr) = run_cuni(
        &[
            "check",
            fixture,
            "--only",
            "py,rs,go,java,sql",
            "--timeout",
            "300",
        ],
        None,
    );
    assert!(
        ok,
        "exactness gate failed for {fixture}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("exactness: PASS"),
        "missing 'exactness: PASS' for {fixture}\n{stdout}"
    );
}

#[test]
fn fee_schedule_gates_on_money_seats() {
    gate_fixture(FEE);
}

#[test]
fn interest_accrual_gates_on_money_seats() {
    gate_fixture(INTEREST);
}

#[test]
fn withholding_gates_on_money_seats() {
    gate_fixture(WITHHOLD);
}

// ---------------------------------------------------------------------------
// 2. Pinned verdicts: the laws cannot silently change.
// ---------------------------------------------------------------------------

fn pinned_verdicts(fixture: &str, expected: &str) {
    let (ok, stdout, stderr) = run_cuni(&["run", fixture], None);
    assert!(ok, "cuni run failed for {fixture}\nstdout:\n{stdout}\nstderr:\n{stderr}");
    assert_eq!(
        stdout, expected,
        "verdicts drifted for {fixture} — the law changed without the gate catching it"
    );
}

#[test]
fn fee_schedule_verdicts_are_pinned() {
    pinned_verdicts(FEE, FEE_VERDICTS);
}

#[test]
fn interest_accrual_verdicts_are_pinned() {
    pinned_verdicts(INTEREST, INTEREST_VERDICTS);
}

#[test]
fn withholding_verdicts_are_pinned() {
    pinned_verdicts(WITHHOLD, WITHHOLD_VERDICTS);
}

// ---------------------------------------------------------------------------
// 3. `cuni audit` — keygen, PASS receipts, signatures, REFUSE paths.
// ---------------------------------------------------------------------------

#[test]
fn audit_gen_key_writes_guarded_keypair() {
    let dir = scratch_dir("genkey");
    let (ok, stdout, stderr) = run_cuni(&["audit", "--gen-key", "testauditor"], Some(&dir));
    assert!(ok, "gen-key failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    let key = dir.join("testauditor.key");
    let publ = dir.join("testauditor.pub");
    let secret = std::fs::read(&key).expect("missing .key file");
    assert_eq!(secret.len(), 32, ".key must be 32 raw secret bytes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&key).expect("stat .key").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, ".key must be mode 600, got {mode:o}");
    }
    let pub_hex = std::fs::read_to_string(&publ).expect("missing .pub file");
    assert!(
        pub_hex.len() == 64 && pub_hex.chars().all(|c| c.is_ascii_hexdigit()),
        ".pub must be 64 hex chars of the public key"
    );
    // Key material is never silently replaced: a second gen-key refuses.
    let (ok2, _, stderr2) = run_cuni(&["audit", "--gen-key", "testauditor"], Some(&dir));
    assert!(!ok2, "gen-key overwrote existing key files — it must refuse");
    assert!(
        stderr2.contains("refusing to overwrite"),
        "overwrite refusal lost its message\n{stderr2}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn gen_key_in(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let (ok, stdout, stderr) = run_cuni(&["audit", "--gen-key", name], Some(dir));
    assert!(ok, "gen-key failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    (dir.join(format!("{name}.key")), dir.join(format!("{name}.pub")))
}

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("failed to write scratch file");
}

/// Run `cuni audit law --against impl [--signer key] --out receipt`, parse
/// the receipt, and return (exit_ok, receipt_json_value).
fn run_audit(
    dir: &Path,
    law: &str,
    impl_name: &str,
    impl_src: &str,
    key: Option<&Path>,
) -> (bool, serde_json::Value, String) {
    let impl_path = dir.join(impl_name);
    write(&impl_path, impl_src);
    let receipt_path = dir.join("receipt.json");
    let mut args: Vec<&str> = vec![
        "audit",
        law,
        "--against",
        impl_path.to_str().unwrap(),
        "--out",
        receipt_path.to_str().unwrap(),
    ];
    let key_str;
    if let Some(k) = key {
        key_str = k.to_str().unwrap().to_string();
        args.push("--signer");
        args.push(&key_str);
    }
    let (ok, stdout, stderr) = run_cuni(&args, None);
    let json_text =
        std::fs::read_to_string(&receipt_path).expect("audit must always file a receipt");
    let v: serde_json::Value =
        serde_json::from_str(&json_text).expect("receipt must be valid JSON");
    assert!(
        stderr.contains("receipt filed at"),
        "audit should announce the filed receipt\n{stderr}"
    );
    let _ = stdout;
    (ok, v, json_text)
}

fn law_path(fixture: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.join(fixture).to_str().unwrap().to_string()
}

fn assert_receipt_hashes(v: &serde_json::Value, law_src: &str, impl_src: &str, gold: &str) {
    let law_bytes = std::fs::read(law_src).expect("read law fixture");
    assert_eq!(v["law_sha256"].as_str().unwrap(), sha256_hex(&law_bytes));
    assert_eq!(
        v["impl_sha256"].as_str().unwrap(),
        sha256_hex(impl_src.as_bytes())
    );
    assert_eq!(
        v["gold_stdout_sha256"].as_str().unwrap(),
        sha256_hex(gold.as_bytes())
    );
    assert!(is_hex64(v["law_sha256"].as_str().unwrap()));
}

#[test]
fn audit_pass_py_impl_with_signature() {
    // The core product: auditor's key, bank's Python, signed PASS receipt.
    let dir = scratch_dir("audit_py");
    let (key, _) = gen_key_in(&dir, "auditor");
    let law = law_path(FEE);
    let (ok, v, json_text) = run_audit(&dir, &law, "bank_fee.py", FEE_PY_IMPL, Some(&key));
    assert!(ok, "audit of a matching impl must exit 0, verdict={}", v["verdict"]);
    assert_eq!(v["verdict"], "PASS");
    assert_receipt_hashes(&v, &law, FEE_PY_IMPL, FEE_VERDICTS);
    let seats: Vec<&str> = v["seats_run"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    for seat in ["py", "rs", "go", "java", "sql"] {
        assert!(seats.contains(&seat), "gold gate must run {seat}; ran {seats:?}");
    }
    assert!(is_iso8601_utc(v["timestamp_utc"].as_str().unwrap()));
    assert_eq!(v["cuni_version"].as_str().unwrap(), env!("CARGO_PKG_VERSION"));
    // The signature re-verifies from the JSON bytes alone.
    assert!(
        v["signer_pubkey"].as_str().is_some_and(|s| s.len() == 64),
        "signed receipt must carry signer_pubkey"
    );
    assert!(
        v["signature"].as_str().is_some_and(|s| s.len() == 128),
        "signed receipt must carry a 128-hex-char Ed25519 signature"
    );
    assert_eq!(
        verify_receipt(&json_text),
        Ok(true),
        "receipt signature must re-verify"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audit_pass_java_impl() {
    // Exercises the javac+java foreign-impl runner.
    let dir = scratch_dir("audit_java");
    let law = law_path(WITHHOLD);
    let (ok, v, _) = run_audit(&dir, &law, "Main.java", WITHHOLD_JAVA_IMPL, None);
    assert!(ok, "audit of a matching Java impl must exit 0, verdict={}", v["verdict"]);
    assert_eq!(v["verdict"], "PASS");
    assert_receipt_hashes(&v, &law, WITHHOLD_JAVA_IMPL, WITHHOLD_VERDICTS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audit_pass_rs_impl() {
    // Exercises the rustc+exec foreign-impl runner.
    let dir = scratch_dir("audit_rs");
    let law = law_path(INTEREST);
    let (ok, v, _) = run_audit(&dir, &law, "impl_accrue.rs", INTEREST_RS_IMPL, None);
    assert!(ok, "audit of a matching Rust impl must exit 0, verdict={}", v["verdict"]);
    assert_eq!(v["verdict"], "PASS");
    assert_receipt_hashes(&v, &law, INTEREST_RS_IMPL, INTEREST_VERDICTS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audit_pass_sql_impl_unsigned() {
    // Exercises the sqlite3 foreign-impl runner; unsigned receipts verify
    // as Ok(false) — the hashes stand on their own.
    let dir = scratch_dir("audit_sql");
    let law = law_path(FEE);
    let (ok, v, json_text) = run_audit(&dir, &law, "bank_fee.sql", FEE_SQL_IMPL, None);
    assert!(ok, "audit of a matching SQL impl must exit 0, verdict={}", v["verdict"]);
    assert_eq!(v["verdict"], "PASS");
    assert!(v.get("signature").is_none(), "unsigned receipt must omit signature");
    assert!(v.get("signer_pubkey").is_none(), "unsigned receipt must omit signer_pubkey");
    assert_eq!(verify_receipt(&json_text), Ok(false), "unsigned receipt verifies as Ok(false)");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audit_tampered_impl_refuses() {
    // One output byte changed: nonzero exit, REFUSE receipt still filed.
    let dir = scratch_dir("audit_tamper");
    let (key, _) = gen_key_in(&dir, "auditor");
    let tampered = FEE_PY_IMPL.replacen("print(\"1.25\")", "print(\"1.26\")", 1);
    assert_ne!(tampered, FEE_PY_IMPL);
    let law = law_path(FEE);
    let (ok, v, json_text) = run_audit(&dir, &law, "bank_fee.py", &tampered, Some(&key));
    assert!(!ok, "audit of a tampered impl must exit nonzero");
    assert!(
        v["verdict"].as_str().unwrap().starts_with("REFUSE"),
        "tampered impl must yield a REFUSE verdict, got {}",
        v["verdict"]
    );
    assert!(
        v["verdict"].as_str().unwrap().contains("diverged"),
        "refusal should name the divergence: {}",
        v["verdict"]
    );
    // A REFUSE receipt is still signed and still re-verifies: refusals are
    // fileable too.
    assert_eq!(verify_receipt(&json_text), Ok(true));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audit_failed_gold_gate_refuses() {
    // A law the front-end refuses: the gold gate fails, so the receipt is
    // REFUSE — never a pass — with no gold stdout to hash.
    let dir = scratch_dir("audit_badgate");
    let bad_law = dir.join("badlaw.cuni");
    write(&bad_law, "say(1.5dec + 1)\n");
    let (ok, v, _) = run_audit(
        &dir,
        bad_law.to_str().unwrap(),
        "bank_fee.py",
        FEE_PY_IMPL,
        None,
    );
    assert!(!ok, "audit of a refused law must exit nonzero");
    assert!(
        v["verdict"].as_str().unwrap().starts_with("REFUSE"),
        "failed gate must yield REFUSE, got {}",
        v["verdict"]
    );
    assert!(
        v["verdict"].as_str().unwrap().contains("gold gate"),
        "refusal should name the gold gate: {}",
        v["verdict"]
    );
    assert!(
        v["gold_stdout_sha256"].is_null(),
        "no gold stdout exists when the gate fails"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audit_unknown_impl_extension_is_refused() {
    let dir = scratch_dir("audit_badext");
    let law = law_path(FEE);
    let (ok, v, _) = run_audit(&dir, &law, "bank_fee.exe", "not a program", None);
    assert!(!ok, "unknown impl seat must exit nonzero");
    assert!(
        v["verdict"].as_str().unwrap().starts_with("REFUSE"),
        "unknown impl seat must REFUSE, got {}",
        v["verdict"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_receipt_rejects_tampering() {
    // Unit-level: flip one hex char of a signed receipt's verdict and the
    // signature must fail — the receipt is tamper-evident.
    let dir = scratch_dir("verify_tamper");
    let (key, _) = gen_key_in(&dir, "auditor");
    let law = law_path(FEE);
    let (ok, _, json_text) = run_audit(&dir, &law, "bank_fee.py", FEE_PY_IMPL, Some(&key));
    assert!(ok);
    assert_eq!(verify_receipt(&json_text), Ok(true));
    let tampered = json_text.replacen("\"verdict\":\"PASS\"", "\"verdict\":\"REFUSE\"", 1);
    assert_ne!(tampered, json_text);
    assert!(
        verify_receipt(&tampered).is_err(),
        "tampered receipt must fail verification"
    );
    // Garbage is an error, not a verdict.
    assert!(verify_receipt("not json").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
