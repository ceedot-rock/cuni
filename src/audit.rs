//! `cuni audit` — the Financial Division's proof-of-compliance command.
//!
//! An auditor (or regulator, or bank) states a financial law once as
//! `.cuni`. The gold gate proves the law itself is exact across the money
//! seats; a foreign implementation (the bank's Java, the auditor's Python)
//! then runs, and its stdout must be byte-identical to gold. Either both
//! hold and the verdict is PASS, or the receipt says REFUSE — a failed gate
//! always emits a REFUSE receipt, never a pass.
//!
//! The receipt is a signed JSON document: hashes of law, impl, and gold
//! stdout, the seats that ran, the verdict, a UTC timestamp, and an
//! optional Ed25519 signature. [`verify_receipt`] re-verifies a receipt
//! (including its signature) from the JSON bytes alone.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Seats the gold gate runs for financial laws: the money seats. `py` is
/// mandatory — the gold stdout is defined as the py seat's stdout
/// (`check::gold_stdout`).
pub const GOLD_SEATS: &[&str] = &["py", "rs", "go", "java", "sql"];

/// A successful gold gate: the seats that ran and the byte-exact gold stdout.
#[derive(Debug, Clone)]
pub struct GoldGate {
    pub seats_run: Vec<String>,
    pub gold_stdout: Vec<u8>,
}

/// A failed gold gate: why, plus the seats that did manage to run (may be
/// empty). A failed gate still emits a REFUSE receipt — never a pass.
#[derive(Debug, Clone)]
pub struct GoldGateError {
    pub reason: String,
    pub seats_run: Vec<String>,
}

/// The signed audit receipt. Field order is the canonical signing order:
/// [`canonical_receipt_bytes`] serializes with `signature: None`, so the
/// signature covers every other field exactly once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub law_sha256: String,
    pub impl_sha256: String,
    /// Hex SHA-256 of the gold stdout. `None` when the gold gate itself
    /// failed (there is no gold stdout to hash).
    pub gold_stdout_sha256: Option<String>,
    pub seats_run: Vec<String>,
    pub verdict: String,
    pub timestamp_utc: String,
    pub cuni_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_pubkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl Receipt {
    pub fn passed(&self) -> bool {
        self.verdict == "PASS"
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_of(&h.finalize())
}

fn hex_of(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("hex string has odd length".into());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16)
            .map_err(|_| format!("invalid hex at offset {i}"))?;
        out.push(byte);
    }
    Ok(out)
}

/// UTC timestamp as ISO-8601 `YYYY-MM-DDTHH:MM:SSZ`, computed from
/// `SystemTime` with a civil-from-days conversion (Hinnant's algorithm —
/// the same one the `time` seat uses to print instants). No time crate.
pub fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Floor-correct split for negative epochs (not reachable from
    // SystemTime, but the conversion is total anyway).
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    // Hinnant civil_from_days.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        m,
        d,
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

/// Canonical receipt bytes: compact JSON with `signature` removed. The
/// signature is computed over exactly these bytes, and [`verify_receipt`]
/// recomputes them the same way. `signer_pubkey` is INCLUDED in the signed
/// bytes (set before signing): the signature binds the key identity to the
/// receipt, so a swapped pubkey invalidates the signature.
pub fn canonical_receipt_bytes(receipt: &Receipt) -> Vec<u8> {
    let unsigned = Receipt {
        signature: None,
        ..receipt.clone()
    };
    serde_json::to_string(&unsigned)
        .expect("receipt serialization cannot fail")
        .into_bytes()
}

/// Generate an Ed25519 keypair for receipt signing.
///
/// Writes `<name>.key` (raw 32 secret bytes, mode 600) and `<name>.pub`
/// (64-char lowercase hex of the public key) into `dir`. Refuses to
/// overwrite existing key files — key material is never silently replaced.
pub fn gen_keypair(name: &str, dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    if name.is_empty() {
        return Err("key name must not be empty".into());
    }
    if name
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    {
        return Err(format!(
            "key name `{name}` must be alphanumeric, `-`, or `_`"
        ));
    }
    let key_path = dir.join(format!("{name}.key"));
    let pub_path = dir.join(format!("{name}.pub"));
    if key_path.exists() || pub_path.exists() {
        return Err(format!(
            "refusing to overwrite existing key files for `{name}`"
        ));
    }
    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret).map_err(|e| format!("entropy failure: {e}"))?;
    let signing = SigningKey::from_bytes(&secret);
    let verifying = signing.verifying_key();

    fs::write(&key_path, &secret).map_err(|e| format!("write {}: {e}", key_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("chmod 600 {}: {e}", key_path.display()))?;
    }
    fs::write(&pub_path, hex_of(verifying.as_bytes()))
        .map_err(|e| format!("write {}: {e}", pub_path.display()))?;
    // Best-effort: don't leave secret bytes lying around in our own memory.
    secret.zeroize_on_drop();
    Ok((key_path, pub_path))
}

/// Load a 32-byte raw secret key file (as written by [`gen_keypair`]).
pub fn load_signing_key(path: &Path) -> Result<SigningKey, String> {
    let bytes =
        fs::read(path).map_err(|e| format!("read signer key {}: {e}", path.display()))?;
    if bytes.len() != 32 {
        return Err(format!(
            "signer key {} is {} bytes, expected 32 raw secret bytes",
            path.display(),
            bytes.len()
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(SigningKey::from_bytes(&arr))
}

/// Sign a receipt's canonical bytes with `signing`. Fills `signer_pubkey`
/// (hex) and `signature` (hex Ed25519 over the canonical bytes, which
/// include the pubkey).
pub fn sign_receipt(receipt: &mut Receipt, signing: &SigningKey) {
    receipt.signer_pubkey = Some(hex_of(signing.verifying_key().as_bytes()));
    let msg = canonical_receipt_bytes(receipt);
    let sig: Signature = signing.sign(&msg);
    receipt.signature = Some(hex_of(&sig.to_bytes()));
}

/// Re-verify a receipt from its JSON bytes.
///
/// - `Ok(true)` — a signature is present and verifies against `signer_pubkey`.
/// - `Ok(false)` — the receipt is unsigned (no signature fields); the
///   hashes and verdict stand on their own.
/// - `Err(_)` — a signature is present but invalid, malformed, or the
///   pubkey/signature pair is inconsistent. Tampering fails here.
pub fn verify_receipt(json: &str) -> Result<bool, String> {
    let receipt: Receipt =
        serde_json::from_str(json).map_err(|e| format!("receipt is not valid JSON: {e}"))?;
    match (&receipt.signer_pubkey, &receipt.signature) {
        (None, None) => Ok(false),
        (Some(_), None) => Err("receipt has signer_pubkey but no signature".into()),
        (None, Some(_)) => Err("receipt has signature but no signer_pubkey".into()),
        (Some(pk_hex), Some(sig_hex)) => {
            let pk_bytes = unhex(pk_hex).map_err(|e| format!("bad signer_pubkey: {e}"))?;
            let sig_bytes = unhex(sig_hex).map_err(|e| format!("bad signature: {e}"))?;
            if pk_bytes.len() != 32 {
                return Err("signer_pubkey must be 32 bytes (64 hex chars)".into());
            }
            if sig_bytes.len() != 64 {
                return Err("signature must be 64 bytes (128 hex chars)".into());
            }
            let mut pk_arr = [0u8; 32];
            pk_arr.copy_from_slice(&pk_bytes);
            let mut sig_arr = [0u8; 64];
            sig_arr.copy_from_slice(&sig_bytes);
            let verifying = VerifyingKey::from_bytes(&pk_arr)
                .map_err(|e| format!("invalid ed25519 public key: {e}"))?;
            let sig = Signature::from_bytes(&sig_arr);
            verifying
                .verify(&canonical_receipt_bytes(&receipt), &sig)
                .map(|()| true)
                .map_err(|_| "signature does not verify — receipt was tampered with".to_string())
        }
    }
}

/// Unique-per-invocation scratch dir (same pid+nanos scheme as the CLI's
/// `work_dir`: parallel audits never share a dir).
fn scratch_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos))
}

fn run_blocking(cmd: &str, args: &[String], timeout: Duration) -> Result<Vec<u8>, String> {
    use std::sync::mpsc;
    let cmd = cmd.to_string();
    let cmd_for_err = cmd.clone();
    let args = args.to_vec();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let output = Command::new(&cmd).args(&args).output();
        let _ = tx.send(output);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(output.stdout)
            } else {
                Err(format!(
                    "exit {}\nstderr: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        }
        Ok(Err(e)) => Err(format!("failed to run `{cmd_for_err}`: {e}")),
        Err(_) => Err(format!(
            "timeout after {}s running `{cmd_for_err}`",
            timeout.as_secs()
        )),
    }
}

/// Run a foreign implementation by file extension and return its raw stdout
/// bytes. Shared by `cuni prove` and `cuni audit` — one runner, one
/// behavior.
///
/// Supported: `.py` (python3), `.js`/`.mjs` (node), `.go` (`go run`),
/// `.rs` (rustc + exec), `.java` (javac + `java -cp <dir> <Class>`),
/// `.sql` (sqlite3 against `:memory:`, same shape as the sql gold seat).
/// Anything else is refused.
pub fn run_foreign_impl(impl_path: &Path, timeout: Duration) -> Result<Vec<u8>, String> {
    let ext = impl_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let path_s = impl_path.to_string_lossy().into_owned();
    match ext.as_str() {
        "py" => run_blocking("python3", &[path_s], timeout),
        "js" | "mjs" => run_blocking("node", &[path_s], timeout),
        "go" => run_blocking("go", &["run".to_string(), path_s], timeout),
        "rs" => {
            let dir = scratch_dir("cuni_audit_rs");
            fs::create_dir_all(&dir)
                .map_err(|e| format!("create scratch dir {}: {e}", dir.display()))?;
            let bin = dir.join("impl_bin");
            let bin_s = bin.to_string_lossy().into_owned();
            let r = (|| {
                run_blocking(
                    "rustc",
                    &[
                        "-O".to_string(),
                        "-o".to_string(),
                        bin_s.clone(),
                        path_s.clone(),
                    ],
                    timeout,
                )?;
                run_blocking(&bin_s, &[], timeout)
            })();
            let _ = fs::remove_dir_all(&dir);
            r
        }
        "java" => {
            let stem = impl_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| "java impl needs a file-stem class name".to_string())?;
            if !is_java_ident(stem) {
                return Err(format!(
                    "java impl file stem `{stem}` is not a valid Java class name"
                ));
            }
            let dir = scratch_dir("cuni_audit_java");
            fs::create_dir_all(&dir)
                .map_err(|e| format!("create scratch dir {}: {e}", dir.display()))?;
            let dir_s = dir.to_string_lossy().into_owned();
            let r = (|| {
                run_blocking(
                    "javac",
                    &["-d".to_string(), dir_s.clone(), path_s.clone()],
                    timeout,
                )?;
                run_blocking(
                    "java",
                    &["-cp".to_string(), dir_s.clone(), stem.to_string()],
                    timeout,
                )
            })();
            let _ = fs::remove_dir_all(&dir);
            r
        }
        "sql" => run_blocking(
            "sqlite3",
            &[
                "-batch".to_string(),
                "-noheader".to_string(),
                ":memory:".to_string(),
                format!(".read {path_s}"),
            ],
            timeout,
        ),
        _ => Err(format!("refuse unknown impl seat `.{ext}`")),
    }
}

fn is_java_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Run the full audit: hash both files, run the injected gold gate, run the
/// foreign impl, compare stdout byte-for-byte.
///
/// `gold_gate` is the CLI's wrapper around `check::check_file_only` — the
/// gate machinery is reused, not duplicated here.
///
/// Returns the receipt in all non-catastrophic cases: a failed gate or a
/// diverged/sick impl yields a REFUSE receipt, never a pass. Hard errors
/// (unreadable files, bad signer key) are `Err` — there is nothing truthful
/// to file.
pub fn audit_law(
    law_path: &Path,
    impl_path: &Path,
    signing: Option<&SigningKey>,
    gold_gate: impl Fn(&Path) -> Result<GoldGate, GoldGateError>,
) -> Result<Receipt, String> {
    let law_bytes =
        fs::read(law_path).map_err(|e| format!("read law {}: {e}", law_path.display()))?;
    let impl_bytes =
        fs::read(impl_path).map_err(|e| format!("read impl {}: {e}", impl_path.display()))?;
    let law_sha256 = sha256_hex(&law_bytes);
    let impl_sha256 = sha256_hex(&impl_bytes);

    let mut receipt = Receipt {
        law_sha256,
        impl_sha256,
        gold_stdout_sha256: None,
        seats_run: Vec::new(),
        verdict: "REFUSE".to_string(),
        timestamp_utc: utc_timestamp(),
        cuni_version: env!("CARGO_PKG_VERSION").to_string(),
        signer_pubkey: None,
        signature: None,
    };

    // (a) The gold gate: the law must be exact across the money seats first.
    // A failed gate always emits a REFUSE receipt — never a pass.
    let gate = match gold_gate(law_path) {
        Ok(g) => g,
        Err(e) => {
            receipt.seats_run = e.seats_run;
            receipt.verdict = format!("REFUSE: gold gate failed: {}", first_line(&e.reason));
            if let Some(signing) = signing {
                sign_receipt(&mut receipt, signing);
            }
            return Ok(receipt);
        }
    };
    receipt.seats_run = gate.seats_run.clone();
    receipt.gold_stdout_sha256 = Some(sha256_hex(&gate.gold_stdout));

    // (b) The foreign impl must print byte-identical stdout to gold.
    match run_foreign_impl(impl_path, Duration::from_secs(120)) {
        Ok(impl_stdout) => {
            if impl_stdout == gate.gold_stdout {
                receipt.verdict = "PASS".to_string();
            } else {
                receipt.verdict = "REFUSE: impl stdout diverged from gold".to_string();
            }
        }
        Err(e) => {
            receipt.verdict = format!("REFUSE: impl failed to run: {}", first_line(&e));
        }
    }

    if let Some(signing) = signing {
        sign_receipt(&mut receipt, signing);
    }
    Ok(receipt)
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

/// `getrandom` 0.2 has no `zeroize` helper; keep the drop narrow and honest:
/// a tiny trait so key bytes are wiped on scope exit.
trait ZeroizeOnDrop {
    fn zeroize_on_drop(&mut self);
}

impl ZeroizeOnDrop for [u8; 32] {
    fn zeroize_on_drop(&mut self) {
        for b in self.iter_mut() {
            *b = 0;
        }
        // Prevent the wipe from being optimized away.
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}
