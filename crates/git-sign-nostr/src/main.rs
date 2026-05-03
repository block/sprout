//! git-sign-nostr — NIP-GS git signing program using Nostr secp256k1 keys.
//!
//! Git invokes this binary as `gpg.x509.program`:
//!
//!   Sign:   `git-sign-nostr --status-fd=<N> -bsau <key>`
//!   Verify: `git-sign-nostr --status-fd=<N> --verify <sig-file> -`
//!
//! Signature format: armored base64 of compact JSON `{"v":1,"pk":...,"sig":...,"t":...}`.
//! Signing hash: SHA-256("nostr:git:v1:" || decimal(t) || ":" || oa_binding || payload).

use std::io::{self, Read, Write};
use std::os::fd::BorrowedFd;
use std::process;
use std::str::FromStr;

use base64::Engine as _;
use nostr::bitcoin::hashes::sha256::Hash as Sha256Hash;
use nostr::bitcoin::hashes::{Hash, HashEngine};
use nostr::bitcoin::secp256k1::schnorr::Signature;
use nostr::bitcoin::secp256k1::{Message, XOnlyPublicKey};
use nostr::{Keys, PublicKey, SECP256K1};
use serde_json::Value;
use zeroize::Zeroizing;

// ── Constants ─────────────────────────────────────────────────────────────────

const MAX_PAYLOAD: usize = 100 * 1024 * 1024; // 100 MB
const MAX_JSON: usize = 2048;
const MAX_B64_LINE: usize = 4096;
const ARMOR_BEGIN: &str = "-----BEGIN SIGNED MESSAGE-----";
const ARMOR_END: &str = "-----END SIGNED MESSAGE-----";
const DOMAIN: &str = "nostr:git:v1:";

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Write an error to stderr and exit 1.
/// Never writes to stdout — git interprets any stdout as signature data.
fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    process::exit(1);
}

/// Run `git config --get <key>`, return trimmed stdout on success.
/// Strips secret env vars from the child process to prevent leakage.
/// Uses bounded piped read to prevent memory exhaustion.
fn git_config(key: &str) -> Option<String> {
    use std::process::Stdio;
    let mut child = process::Command::new("git")
        .args(["config", "--get", key])
        .env_remove("NOSTR_PRIVATE_KEY")
        .env_remove("SPROUT_PRIVATE_KEY")
        .env_remove("SPROUT_AUTH_TAG")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Bounded read: git config values should never exceed 4 KB.
    let stdout = child.stdout.take()?;
    let mut buf = Vec::with_capacity(256);
    if io::Read::read_to_end(&mut stdout.take(4097), &mut buf).is_err() {
        let _ = child.kill();
        return None;
    }
    if buf.len() > 4096 {
        let _ = child.kill();
        return None;
    }

    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).trim().to_string())
}

#[cfg(unix)]
fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    // SAFETY: getuid(2) has no preconditions; it is always safe to call.
    unsafe { getuid() }
}

/// Load the private key: NOSTR_PRIVATE_KEY → SPROUT_PRIVATE_KEY → keyfile.
/// Returns the raw key string (nsec or hex) wrapped in Zeroizing for automatic cleanup.
fn load_key() -> Result<Zeroizing<String>, String> {
    for var in &["NOSTR_PRIVATE_KEY", "SPROUT_PRIVATE_KEY"] {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(Zeroizing::new(trimmed));
            }
            // val goes out of scope here — can't zeroize env strings, but
            // trimmed is wrapped in Zeroizing.
        }
    }
    let path = git_config("nostr.keyfile").ok_or_else(|| {
        "no nostr key configured. Set $NOSTR_PRIVATE_KEY, $SPROUT_PRIVATE_KEY, or git config nostr.keyfile".to_string()
    })?;
    read_keyfile_secure(&path).map(Zeroizing::new)
}

/// Open keyfile with O_NOFOLLOW, verify permissions on the open fd (no TOCTOU),
/// then read the content. Returns trimmed key string.
#[cfg(unix)]
fn read_keyfile_secure(path: &str) -> Result<String, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    // Open with O_NOFOLLOW to reject symlinks atomically.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc_o_nofollow())
        .open(path)
        .map_err(|e| format!("cannot open keyfile {path}: {e}"))?;

    // fstat on the open fd — no TOCTOU gap.
    let meta = file
        .metadata()
        .map_err(|e| format!("cannot stat keyfile {path}: {e}"))?;

    if !meta.file_type().is_file() {
        return Err(format!("keyfile {path} must be a regular file"));
    }
    let current_uid = libc_getuid();
    if meta.uid() != current_uid {
        return Err(format!(
            "keyfile {path} is not owned by the current user (uid {current_uid})"
        ));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o177 != 0 {
        return Err(format!(
            "keyfile {path} has insecure permissions (expected 0600, got {mode:04o})"
        ));
    }

    // Read from the already-open fd (bounded, with truncation detection).
    let mut contents = String::new();
    use std::io::Read;
    io::Read::read_to_string(&mut io::BufReader::new(file).take(4097), &mut contents)
        .map_err(|e| format!("cannot read keyfile {path}: {e}"))?;
    if contents.len() > 4096 {
        return Err(format!("keyfile {path} is too large (>4096 bytes)"));
    }
    Ok(contents.trim().to_string())
}

#[cfg(target_os = "macos")]
fn libc_o_nofollow() -> i32 {
    0x0100 // O_NOFOLLOW on macOS
}

#[cfg(target_os = "linux")]
fn libc_o_nofollow() -> i32 {
    0o400000 // O_NOFOLLOW on Linux
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "linux")))]
fn libc_o_nofollow() -> i32 {
    0o400000 // Best guess for other Unix
}

#[cfg(not(unix))]
fn read_keyfile_secure(path: &str) -> Result<String, String> {
    eprintln!("warning: cannot verify keyfile permissions on this platform ({path})");
    let raw =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read keyfile {path}: {e}"))?;
    Ok(raw.trim().to_string())
}

/// Load optional NIP-OA auth tag JSON: SPROUT_AUTH_TAG env → nostr.authtag git config.
/// Returns the raw JSON string `["auth","<owner>","<cond>","<sig>"]` or None.
fn load_auth_tag() -> Option<String> {
    if let Ok(val) = std::env::var("SPROUT_AUTH_TAG") {
        if !val.is_empty() {
            return Some(val);
        }
    }
    git_config("nostr.authtag")
}

/// Parse and validate a NIP-OA auth tag JSON into `[owner_hex, conditions, owner_sig_hex]`.
/// Expects `["auth", owner_hex_64, conditions, sig_hex_128]`.
///
/// Validates:
///   - owner is 64 lowercase hex chars and a valid BIP-340 x-only key
///   - sig is 128 lowercase hex chars
///   - conditions contains only safe characters `[a-zA-Z0-9_=<>&]`
fn parse_oa_tag(json: &str) -> Result<[String; 3], String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("invalid auth tag JSON: {e}"))?;
    let arr = v.as_array().ok_or("auth tag must be a JSON array")?;
    if arr.len() != 4 {
        return Err(format!("auth tag must have 4 elements, got {}", arr.len()));
    }
    let label = arr[0].as_str().ok_or("element 0 must be a string")?;
    if label != "auth" {
        return Err(format!("first element must be \"auth\", got {label:?}"));
    }
    let owner = arr[1]
        .as_str()
        .ok_or("element 1 must be a string")?
        .to_string();
    let cond = arr[2]
        .as_str()
        .ok_or("element 2 must be a string")?
        .to_string();
    let sig = arr[3]
        .as_str()
        .ok_or("element 3 must be a string")?
        .to_string();

    // Validate owner pubkey format and validity.
    if !is_lower_hex(&owner, 64) {
        return Err("auth tag owner must be 64 lowercase hex chars".to_string());
    }
    PublicKey::from_hex(&owner)
        .map_err(|e| format!("auth tag owner is not a valid BIP-340 key: {e}"))?;

    // Validate owner sig format.
    if !is_lower_hex(&sig, 128) {
        return Err("auth tag sig must be 128 lowercase hex chars".to_string());
    }

    // Validate conditions: must be empty or valid NIP-OA clauses separated by '&'.
    // Valid clauses: "kind=<u16>", "created_at<N", "created_at>N" where N is a u64 timestamp.
    if !cond.is_empty() {
        validate_oa_conditions(&cond)?;
    }

    Ok([owner, cond, sig])
}

/// Validate NIP-OA conditions string: clauses separated by '&'.
/// Valid clauses: "kind=<u16>", "created_at<N", "created_at>N" (N = decimal u64, no leading zeros).
fn validate_oa_conditions(cond: &str) -> Result<(), String> {
    if cond.is_empty() {
        return Err("conditions string is empty (use empty string for no conditions)".to_string());
    }
    for clause in cond.split('&') {
        if clause.is_empty() {
            return Err("empty clause in conditions (double '&' or trailing '&')".to_string());
        }
        if let Some(val) = clause.strip_prefix("kind=") {
            // Must be a valid u16 with no leading zeros.
            if val.starts_with('0') && val.len() > 1 {
                return Err(format!("kind value has leading zeros: {clause:?}"));
            }
            val.parse::<u16>()
                .map_err(|_| format!("invalid kind value in conditions: {clause:?}"))?;
        } else if let Some(val) = clause.strip_prefix("created_at<") {
            if val.starts_with('0') && val.len() > 1 {
                return Err(format!("created_at value has leading zeros: {clause:?}"));
            }
            let n = val
                .parse::<u64>()
                .map_err(|_| format!("invalid created_at< value: {clause:?}"))?;
            if n > u32::MAX as u64 {
                return Err(format!("created_at value exceeds u32 range: {clause:?}"));
            }
        } else if let Some(val) = clause.strip_prefix("created_at>") {
            if val.starts_with('0') && val.len() > 1 {
                return Err(format!("created_at value has leading zeros: {clause:?}"));
            }
            let n = val
                .parse::<u64>()
                .map_err(|_| format!("invalid created_at> value: {clause:?}"))?;
            if n > u32::MAX as u64 {
                return Err(format!("created_at value exceeds u32 range: {clause:?}"));
            }
        } else {
            return Err(format!("unrecognized condition clause: {clause:?}"));
        }
    }
    Ok(())
}

/// Check that a string is exactly `len` lowercase hex characters.
fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}

/// Compute the NIP-GS signing hash and return the secp256k1 Message.
///
/// hash = SHA-256("nostr:git:v1:" || decimal(t) || ":" || oa_binding || payload)
///
/// Uses incremental hashing to avoid duplicating the payload in memory.
fn signing_message(t: u64, oa: Option<&[String; 3]>, payload: &[u8]) -> Message {
    let mut engine = Sha256Hash::engine();
    engine.input(DOMAIN.as_bytes());
    engine.input(t.to_string().as_bytes());
    engine.input(b":");
    if let Some(oa) = oa {
        // oa_binding = oa[0] || ":" || oa[1] || ":" || oa[2] || ":"
        engine.input(oa[0].as_bytes());
        engine.input(b":");
        engine.input(oa[1].as_bytes());
        engine.input(b":");
        engine.input(oa[2].as_bytes());
        engine.input(b":");
    }
    engine.input(payload);
    let digest = Sha256Hash::from_engine(engine);
    Message::from_digest(digest.to_byte_array())
}

/// Format a unix timestamp as `YYYY-MM-DD` (UTC) without external crates.
fn format_date(t: u64) -> String {
    // Days since Unix epoch using the proleptic Gregorian calendar.
    let days = t / 86400;
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Open a writable file from a raw file descriptor number.
///
/// Validates the fd is actually open via `fcntl(F_GETFD)` before borrowing,
/// then duplicates via `try_clone_to_owned` so we never close git's original fd.
fn open_status_fd(fd: i32) -> Box<dyn Write> {
    if fd < 0 {
        return Box::new(io::stderr());
    }
    // Validate the fd is actually open before creating a BorrowedFd.
    if !is_valid_fd(fd) {
        eprintln!("warning: status-fd {fd} is not a valid open fd, using stderr");
        return Box::new(io::stderr());
    }
    // SAFETY: `fd` is a valid open fd (validated above via fcntl F_GETFD).
    // BorrowedFd borrows without taking ownership; try_clone_to_owned
    // calls dup(2) and returns a new OwnedFd we exclusively own.
    let owned = unsafe { BorrowedFd::borrow_raw(fd) }
        .try_clone_to_owned()
        .unwrap_or_else(|_| {
            eprintln!("warning: dup({fd}) failed, using stderr");
            // Validate fd 2 before borrowing — if stderr is closed, exit.
            if !is_valid_fd(2) {
                process::exit(1);
            }
            // Fall back to dup(2) on stderr. If even this fails, abort —
            // we have no fd to write status lines to.
            match unsafe { BorrowedFd::borrow_raw(2) }.try_clone_to_owned() {
                Ok(fd) => fd,
                Err(_) => {
                    // No usable fd at all. Exit silently — git will see the
                    // missing status output and report a signing failure.
                    process::exit(1);
                }
            }
        });
    Box::new(std::fs::File::from(owned))
}

/// Check if a file descriptor is valid (open) using fcntl(2).
#[cfg(unix)]
fn is_valid_fd(fd: i32) -> bool {
    extern "C" {
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
    const F_GETFD: i32 = 1;
    // SAFETY: fcntl(fd, F_GETFD) has no side effects; returns -1 if fd is invalid.
    unsafe { fcntl(fd, F_GETFD) != -1 }
}

#[cfg(not(unix))]
fn is_valid_fd(_fd: i32) -> bool {
    true // Non-Unix: assume valid, will fail on write instead.
}

/// Write a `[GNUPG:] ` status line to the status fd.
macro_rules! status {
    ($fd:expr, $($arg:tt)*) => {{
        let line = format!("[GNUPG:] {}\n", format_args!($($arg)*));
        let _ = $fd.write_all(line.as_bytes());
    }};
}

// ── Argument parsing ──────────────────────────────────────────────────────────

#[derive(Debug)]
enum Mode {
    Sign { key_arg: String },
    Verify { sig_file: String },
}

struct Args {
    status_fd: i32,
    mode: Mode,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut status_fd: i32 = 2; // default: stderr
    let mut mode: Option<Mode> = None;
    let mut i = 0;

    while i < raw.len() {
        let arg = &raw[i];

        // --status-fd=N  or  --status-fd N
        if let Some(n) = arg.strip_prefix("--status-fd=") {
            status_fd = n.parse().unwrap_or(2);
            i += 1;
            continue;
        }
        if arg == "--status-fd" {
            if let Some(n) = raw.get(i + 1) {
                status_fd = n.parse().unwrap_or(2);
                i += 2;
                continue;
            }
        }

        // --verify <file> -
        if arg == "--verify" {
            if let Some(file) = raw.get(i + 1) {
                mode = Some(Mode::Verify {
                    sig_file: file.clone(),
                });
                i += 3; // skip file and trailing "-"
                continue;
            }
        }

        // -bsau <key>  (git passes these as a single arg "-bsau" then the key)
        if arg == "-bsau" {
            let key = raw.get(i + 1).cloned().unwrap_or_default();
            mode = Some(Mode::Sign { key_arg: key });
            i += 2;
            continue;
        }

        // Silently ignore unrecognized flags for forward compatibility.
        i += 1;
    }

    let mode = mode.unwrap_or_else(|| fail("no mode specified (expected -bsau or --verify)"));
    Args { status_fd, mode }
}

// ── Sign ──────────────────────────────────────────────────────────────────────

fn cmd_sign(key_arg: &str, status_fd: i32) {
    // Load key material (Zeroizing wrapper auto-clears on drop).
    let raw_key = match load_key() {
        Ok(k) => k,
        Err(e) => fail(&e),
    };
    let keys = match Keys::parse(&*raw_key) {
        Ok(k) => k,
        Err(e) => {
            // raw_key drops here (Zeroizing clears it automatically)
            fail(&format!("invalid nostr private key: {e}"));
        }
    };
    drop(raw_key); // Explicit drop triggers zeroization immediately.

    // Validate -u key argument matches loaded key.
    // Fail closed: if -u is non-empty but unparseable, reject rather than
    // silently signing with a potentially wrong key.
    if !key_arg.is_empty() {
        match PublicKey::parse(key_arg) {
            Ok(expected) => {
                if expected != keys.public_key() {
                    fail(&format!(
                        "signing key mismatch: -u specifies {}, but loaded key is {}",
                        expected.to_hex(),
                        keys.public_key().to_hex()
                    ));
                }
            }
            Err(_) => {
                // -u arg is not a recognizable key format. Fail closed to
                // prevent signing with an unintended key after typos.
                fail(&format!(
                    "signing key -u argument is not a valid key identifier: {key_arg:?}"
                ));
            }
        }
    }

    // Load NIP-OA auth tag. If explicitly configured but invalid, fail closed —
    // an agent should not sign without proper authorization when one is expected.
    let pk_hex = keys.public_key().to_hex();
    let oa: Option<[String; 3]> = match load_auth_tag() {
        Some(json) => match parse_oa_tag(&json) {
            Ok(tag) => {
                // Reject self-attestation: owner must differ from signer.
                if tag[0] == pk_hex {
                    fail("auth tag is self-attestation (owner == signer)");
                }
                // Verify the OA actually authorizes this signing key.
                if let Err(e) = verify_oa(&tag, &keys.public_key()) {
                    fail(&format!("auth tag does not authorize this key: {e}"));
                }
                Some(tag)
            }
            Err(e) => {
                fail(&format!("malformed auth tag: {e}"));
            }
        },
        None => None, // No auth tag configured — signing without OA is fine.
    };

    // Read payload from stdin (max 100 MB).
    let payload = read_payload_stdin();

    // Timestamp — fail if clock is broken or exceeds NIP-GS u32 range.
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|_| fail("system clock is before Unix epoch"));
    if t > u32::MAX as u64 {
        fail("system clock exceeds NIP-GS u32 timestamp range (year 2106+)");
    }

    // Enforce OA temporal conditions before signing — don't produce signatures
    // with expired or not-yet-valid delegations.
    if let Some(ref oa) = oa {
        if !evaluate_oa_conditions(&oa[1], t) {
            fail("auth tag temporal conditions not satisfied at current time");
        }
    }

    // Compute signing hash and sign.
    let message = signing_message(t, oa.as_ref(), &payload);
    let sig = keys.sign_schnorr(&message);
    let pk_hex = keys.public_key().to_hex();
    let sig_hex = sig.to_string();

    // Build compact JSON with required field order: v, pk, sig, t, [oa].
    let json = if let Some(ref oa) = oa {
        format!(
            r#"{{"v":1,"pk":"{pk_hex}","sig":"{sig_hex}","t":{t},"oa":["{o}","{c}","{s}"]}}"#,
            o = oa[0],
            c = oa[1],
            s = oa[2],
        )
    } else {
        format!(r#"{{"v":1,"pk":"{pk_hex}","sig":"{sig_hex}","t":{t}}}"#)
    };

    // Armor and write to stdout.
    let b64 = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
    println!("{ARMOR_BEGIN}");
    println!("{b64}");
    println!("{ARMOR_END}");
    let _ = io::stdout().flush();

    // Write status lines.
    let mut sfd = open_status_fd(status_fd);
    status!(sfd, "BEGIN_SIGNING");
    status!(sfd, "SIG_CREATED D 8 1 00 {t} {pk_hex}");
}

// ── Verify ────────────────────────────────────────────────────────────────────

fn cmd_verify(sig_file: &str, status_fd: i32) {
    let mut sfd = open_status_fd(status_fd);

    // Helper: emit ERRSIG and exit 1.
    let errsig = |sfd: &mut Box<dyn Write>, key_id: &str, msg: &str| -> ! {
        eprintln!("error: {msg}");
        status!(sfd, "ERRSIG {key_id} 0 0 00 0 9");
        process::exit(1);
    };

    // Read and parse the signature file with bounded read (prevents memory DoS).
    // A valid NIP-GS signature is ~300 bytes armored; cap at 64 KB.
    const MAX_SIG_FILE: u64 = 64 * 1024;
    let sig_bytes = match std::fs::File::open(sig_file) {
        Ok(f) => {
            let mut buf = Vec::with_capacity(4096);
            match io::Read::read_to_end(&mut f.take(MAX_SIG_FILE + 1), &mut buf) {
                Ok(_) if buf.len() as u64 > MAX_SIG_FILE => errsig(
                    &mut sfd,
                    "0000000000000000",
                    &format!("signature file too large (>{MAX_SIG_FILE} bytes)"),
                ),
                Ok(_) => buf,
                Err(e) => errsig(
                    &mut sfd,
                    "0000000000000000",
                    &format!("cannot read signature file: {e}"),
                ),
            }
        }
        Err(e) => errsig(
            &mut sfd,
            "0000000000000000",
            &format!("cannot open signature file: {e}"),
        ),
    };
    let sig_text = match std::str::from_utf8(&sig_bytes) {
        Ok(s) => s,
        Err(_) => errsig(
            &mut sfd,
            "0000000000000000",
            "signature file is not valid UTF-8",
        ),
    };

    // Parse armor: exactly BEGIN\nb64\nEND\n (trailing \n optional).
    let (b64_line, _) = match parse_armor(sig_text) {
        Ok(v) => v,
        Err(e) => errsig(&mut sfd, "0000000000000000", &e),
    };

    // Decode base64.
    let json_bytes = match base64::engine::general_purpose::STANDARD.decode(b64_line) {
        Ok(b) => b,
        Err(e) => errsig(
            &mut sfd,
            "0000000000000000",
            &format!("base64 decode failed: {e}"),
        ),
    };
    if json_bytes.len() > MAX_JSON {
        errsig(
            &mut sfd,
            "0000000000000000",
            "decoded JSON exceeds 2048 bytes",
        );
    }
    let json_str = match std::str::from_utf8(&json_bytes) {
        Ok(s) => s,
        Err(_) => errsig(
            &mut sfd,
            "0000000000000000",
            "decoded JSON is not valid UTF-8",
        ),
    };

    // Parse and validate the envelope.
    let env = match parse_envelope(json_str) {
        Ok(e) => e,
        Err((key_id, msg)) => errsig(&mut sfd, &key_id, &msg),
    };

    // Canonical JSON check: reconstruct and compare byte-for-byte.
    let canonical = build_canonical_json(&env);
    if canonical.as_bytes() != json_bytes {
        errsig(
            &mut sfd,
            &env.pk,
            "non-canonical JSON (field order, whitespace, or number format)",
        );
    }

    // Read payload from stdin (max 100 MB).
    let payload = read_payload_stdin_errsig(&mut sfd, &env.pk);

    // Compute signing hash and verify BIP-340 signature.
    let message = signing_message(env.t, env.oa.as_ref(), &payload);
    let sig = match Signature::from_str(&env.sig) {
        Ok(s) => s,
        Err(e) => errsig(&mut sfd, &env.pk, &format!("invalid signature hex: {e}")),
    };
    let pk = match PublicKey::from_hex(&env.pk) {
        Ok(p) => p,
        Err(e) => errsig(&mut sfd, &env.pk, &format!("invalid public key: {e}")),
    };
    let xonly: &XOnlyPublicKey = &pk;

    status!(sfd, "NEWSIG");

    if SECP256K1.verify_schnorr(&sig, &message, xonly).is_err() {
        status!(sfd, "BADSIG {pk_hex} {pk_hex}", pk_hex = env.pk);
        eprintln!("error: signature verification failed");
        process::exit(1);
    }

    // Verify NIP-OA attestation if present.
    // If OA is present but invalid, or conditions are violated, downgrade trust.
    let oa_valid = if let Some(ref oa) = env.oa {
        match verify_oa(oa, &pk) {
            Ok(()) => {
                // Also evaluate temporal conditions against the signature timestamp.
                evaluate_oa_conditions(&oa[1], env.t)
            }
            Err(e) => {
                eprintln!("warning: owner attestation invalid: {e}");
                false
            }
        }
    } else {
        true // No OA present is fine — it's optional.
    };

    // Determine trust level:
    // - Invalid OA → TRUST_UNDEFINED (regardless of key match)
    // - user.signingkey matches signer pk → TRUST_FULLY
    // - user.signingkey matches OA owner (valid delegation) → TRUST_FULLY
    // - Otherwise → TRUST_UNDEFINED
    let trust = if !oa_valid {
        "TRUST_UNDEFINED"
    } else {
        match git_config("user.signingkey") {
            Some(configured) => {
                let configured_hex = PublicKey::parse(&configured)
                    .map(|p| p.to_hex())
                    .unwrap_or_else(|_| configured.to_lowercase());
                if configured_hex.eq_ignore_ascii_case(&env.pk) {
                    // Direct key match — signer is the configured key.
                    "TRUST_FULLY"
                } else if let Some(ref oa) = env.oa {
                    // Check if configured key matches the OA owner —
                    // valid delegation from a trusted owner.
                    if oa_valid && configured_hex.eq_ignore_ascii_case(&oa[0]) {
                        "TRUST_FULLY"
                    } else {
                        "TRUST_UNDEFINED"
                    }
                } else {
                    "TRUST_UNDEFINED"
                }
            }
            None => "TRUST_UNDEFINED",
        }
    };

    let date = format_date(env.t);
    status!(sfd, "GOODSIG {pk} {pk}", pk = env.pk);
    status!(
        sfd,
        "VALIDSIG {fpr} {date} {t} 0 - - - - - {fpr}",
        fpr = env.pk,
        date = date,
        t = env.t
    );
    status!(sfd, "{trust} 0 shell");
}

// ── Envelope ──────────────────────────────────────────────────────────────────

struct Envelope {
    pk: String,
    sig: String,
    t: u64,
    oa: Option<[String; 3]>,
}

/// Parse and validate the NIP-GS JSON envelope.
/// Returns `Err((key_id, message))` on failure.
fn parse_envelope(json_str: &str) -> Result<Envelope, (String, String)> {
    let err =
        |key_id: &str, msg: &str| -> (String, String) { (key_id.to_string(), msg.to_string()) };

    // Note: serde_json::Value silently keeps the last value for duplicate keys.
    // Duplicate-key attacks are mitigated by the canonical JSON round-trip check
    // performed after this function returns.
    let v: Value = serde_json::from_str(json_str)
        .map_err(|e| err("0000000000000000", &format!("JSON parse error: {e}")))?;

    let obj = v
        .as_object()
        .ok_or_else(|| err("0000000000000000", "JSON root must be an object"))?;

    // Reject unknown keys (for v=1).
    for key in obj.keys() {
        if !matches!(key.as_str(), "v" | "pk" | "sig" | "t" | "oa") {
            return Err(err("0000000000000000", &format!("unknown field: {key:?}")));
        }
    }

    // v must be integer 1.
    let v_val = obj
        .get("v")
        .ok_or_else(|| err("0000000000000000", "missing field \"v\""))?;
    if v_val.as_u64() != Some(1) {
        return Err(err("0000000000000000", "\"v\" must be integer 1"));
    }

    // pk: 64 lowercase hex chars, valid BIP-340 key.
    let pk = obj
        .get("pk")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err("0000000000000000", "missing or non-string \"pk\""))?;
    if !is_lower_hex(pk, 64) {
        return Err(err(
            "0000000000000000",
            "\"pk\" must be 64 lowercase hex chars",
        ));
    }
    // Validate it's a real BIP-340 x-only key (lift_x check).
    PublicKey::from_hex(pk).map_err(|e| {
        err(
            "0000000000000000",
            &format!("\"pk\" is not a valid BIP-340 key: {e}"),
        )
    })?;

    // sig: 128 lowercase hex chars.
    let sig = obj
        .get("sig")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(pk, "missing or non-string \"sig\""))?;
    if !is_lower_hex(sig, 128) {
        return Err(err(pk, "\"sig\" must be 128 lowercase hex chars"));
    }

    // t: integer in [0, 4294967295].
    let t_val = obj.get("t").ok_or_else(|| err(pk, "missing field \"t\""))?;
    let t = t_val
        .as_u64()
        .filter(|&n| n <= 4294967295)
        .ok_or_else(|| err(pk, "\"t\" must be an integer in [0, 4294967295]"))?;
    // Reject floats serialized as integers (serde_json parses 1.0 as f64).
    if t_val.is_f64() {
        return Err(err(pk, "\"t\" must not be a float"));
    }

    // oa: optional array of exactly 3 strings.
    let oa = match obj.get("oa") {
        None => None,
        Some(oa_val) => {
            let arr = oa_val
                .as_array()
                .ok_or_else(|| err(pk, "\"oa\" must be an array"))?;
            if arr.len() != 3 {
                return Err(err(
                    pk,
                    &format!("\"oa\" must have 3 elements, got {}", arr.len()),
                ));
            }
            let s0 = arr[0]
                .as_str()
                .ok_or_else(|| err(pk, "oa[0] must be a string"))?;
            let s1 = arr[1]
                .as_str()
                .ok_or_else(|| err(pk, "oa[1] must be a string"))?;
            let s2 = arr[2]
                .as_str()
                .ok_or_else(|| err(pk, "oa[2] must be a string"))?;
            // Validate owner pubkey format.
            if !is_lower_hex(s0, 64) {
                return Err(err(pk, "oa[0] must be 64 lowercase hex chars"));
            }
            // Validate owner pubkey is a real BIP-340 key.
            PublicKey::from_hex(s0)
                .map_err(|e| err(pk, &format!("oa[0] is not a valid BIP-340 key: {e}")))?;
            // Owner must not equal signer (no self-attestation).
            if s0 == pk {
                return Err(err(
                    pk,
                    "oa[0] (owner) must not equal pk (self-attestation rejected)",
                ));
            }
            // Validate owner sig format.
            if !is_lower_hex(s2, 128) {
                return Err(err(pk, "oa[2] must be 128 lowercase hex chars"));
            }
            // Validate conditions using the same NIP-OA grammar as the sign path.
            if !s1.is_empty() {
                if let Err(e) = validate_oa_conditions(s1) {
                    return Err(err(pk, &format!("oa[1] conditions invalid: {e}")));
                }
            }
            Some([s0.to_string(), s1.to_string(), s2.to_string()])
        }
    };

    Ok(Envelope {
        pk: pk.to_string(),
        sig: sig.to_string(),
        t,
        oa,
    })
}

/// Reconstruct the canonical compact JSON for byte-for-byte comparison.
fn build_canonical_json(env: &Envelope) -> String {
    if let Some(ref oa) = env.oa {
        format!(
            r#"{{"v":1,"pk":"{pk}","sig":"{sig}","t":{t},"oa":["{o}","{c}","{s}"]}}"#,
            pk = env.pk,
            sig = env.sig,
            t = env.t,
            o = oa[0],
            c = oa[1],
            s = oa[2],
        )
    } else {
        format!(
            r#"{{"v":1,"pk":"{pk}","sig":"{sig}","t":{t}}}"#,
            pk = env.pk,
            sig = env.sig,
            t = env.t,
        )
    }
}

/// Evaluate NIP-OA conditions against a signature timestamp.
/// Returns true if all conditions pass, false if any are violated.
/// Empty conditions always pass.
fn evaluate_oa_conditions(conditions: &str, t: u64) -> bool {
    if conditions.is_empty() {
        return true;
    }
    for clause in conditions.split('&') {
        if let Some(val) = clause.strip_prefix("created_at<") {
            if let Ok(bound) = val.parse::<u64>() {
                if t >= bound {
                    eprintln!(
                        "warning: OA condition violated: signature t={t} >= created_at<{bound}"
                    );
                    return false;
                }
            }
        } else if let Some(val) = clause.strip_prefix("created_at>") {
            if let Ok(bound) = val.parse::<u64>() {
                if t <= bound {
                    eprintln!(
                        "warning: OA condition violated: signature t={t} <= created_at>{bound}"
                    );
                    return false;
                }
            }
        }
        // kind= conditions restrict Nostr event kinds. Git objects are not
        // Nostr events, so kind= conditions are inapplicable here — skip them
        // per NIP-GS spec (verifiers ignore conditions they don't understand).
        // The OA is still valid for git signing even if it also restricts kinds.
    }
    true
}

/// Verify a NIP-OA attestation: `oa[2]` over SHA-256("nostr:agent-auth:" || pk || ":" || oa[1])
/// against `oa[0]` (owner pubkey), where `pk` is the agent/signer pubkey.
fn verify_oa(oa: &[String; 3], agent_pk: &PublicKey) -> Result<(), String> {
    let preimage = format!("nostr:agent-auth:{}:{}", agent_pk.to_hex(), oa[1]);
    let digest = Sha256Hash::hash(preimage.as_bytes());
    let message = Message::from_digest(digest.to_byte_array());

    let owner_pk = PublicKey::from_hex(&oa[0]).map_err(|e| format!("invalid owner pubkey: {e}"))?;
    let owner_sig = Signature::from_str(&oa[2]).map_err(|e| format!("invalid owner sig: {e}"))?;
    let xonly: &XOnlyPublicKey = &owner_pk;

    SECP256K1
        .verify_schnorr(&owner_sig, &message, xonly)
        .map_err(|e| format!("NIP-OA verification failed: {e}"))
}

// ── Armor parsing ─────────────────────────────────────────────────────────────

/// Parse the armored signature text. Returns `(b64_line, ())` on success.
/// Enforces: exactly BEGIN\nb64\nEND\n, no line wrapping, no CRLF.
fn parse_armor(text: &str) -> Result<(&str, ()), String> {
    // Reject CRLF.
    if text.contains('\r') {
        return Err("signature contains CRLF line endings".into());
    }

    // Strip optional trailing newline after END marker.
    let text = text.strip_suffix('\n').unwrap_or(text);

    let mut lines = text.splitn(3, '\n');
    let begin = lines.next().unwrap_or("");
    let b64 = lines.next().ok_or("missing base64 line in armor")?;
    let end = lines.next().ok_or("missing END marker in armor")?;

    if begin != ARMOR_BEGIN {
        return Err(format!("expected {ARMOR_BEGIN:?}, got {begin:?}"));
    }
    if end != ARMOR_END {
        return Err(format!("expected {ARMOR_END:?}, got {end:?}"));
    }
    // Reject line-wrapped base64 (any embedded newline).
    if b64.contains('\n') {
        return Err("base64 content must not be line-wrapped".into());
    }
    if b64.len() > MAX_B64_LINE {
        return Err(format!("base64 line exceeds {MAX_B64_LINE} bytes"));
    }
    // Reject trailing whitespace.
    if b64 != b64.trim_end() {
        return Err("trailing whitespace on base64 line".into());
    }

    Ok((b64, ()))
}

// ── Stdin reading ─────────────────────────────────────────────────────────────

/// Read at most MAX_PAYLOAD bytes from stdin.
///
/// Uses `take(MAX_PAYLOAD + 1)` so the OS never buffers more than one byte
/// past the limit before we detect the overrun — no unbounded allocation.
fn read_payload_stdin() -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    match io::stdin()
        .take((MAX_PAYLOAD + 1) as u64)
        .read_to_end(&mut buf)
    {
        Ok(_) if buf.len() <= MAX_PAYLOAD => buf,
        Ok(_) => fail("payload exceeds 100 MB limit"),
        Err(e) => fail(&format!("failed to read stdin: {e}")),
    }
}

fn read_payload_stdin_errsig(sfd: &mut Box<dyn Write>, pk: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    match io::stdin()
        .take((MAX_PAYLOAD + 1) as u64)
        .read_to_end(&mut buf)
    {
        Ok(_) if buf.len() <= MAX_PAYLOAD => buf,
        Ok(_) => {
            eprintln!("error: payload exceeds 100 MB limit");
            status!(sfd, "ERRSIG {pk} 0 0 00 0 9");
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: failed to read stdin: {e}");
            status!(sfd, "ERRSIG {pk} 0 0 00 0 9");
            process::exit(1);
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args = parse_args();
    match args.mode {
        Mode::Sign { key_arg } => cmd_sign(&key_arg, args.status_fd),
        Mode::Verify { sig_file } => cmd_verify(&sig_file, args.status_fd),
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_lower_hex ──────────────────────────────────────────────────────────

    #[test]
    fn lower_hex_valid() {
        assert!(is_lower_hex("deadbeef", 8));
        assert!(is_lower_hex(&"a".repeat(64), 64));
        assert!(is_lower_hex(&"0123456789abcdef".repeat(4), 64));
    }

    #[test]
    fn lower_hex_rejects_uppercase() {
        assert!(!is_lower_hex("DEADBEEF", 8));
        assert!(!is_lower_hex("DeadBeef", 8));
    }

    #[test]
    fn lower_hex_rejects_wrong_length() {
        assert!(!is_lower_hex("deadbeef", 7));
        assert!(!is_lower_hex("deadbeef", 9));
        assert!(!is_lower_hex("", 1));
    }

    #[test]
    fn lower_hex_rejects_non_hex() {
        assert!(!is_lower_hex("deadbeeg", 8));
        assert!(!is_lower_hex("dead beef", 9));
    }

    // ── parse_armor ───────────────────────────────────────────────────────────

    #[test]
    fn armor_roundtrip() {
        let b64 = "dGVzdA==";
        let text = format!("{ARMOR_BEGIN}\n{b64}\n{ARMOR_END}\n");
        let (got, _) = parse_armor(&text).unwrap();
        assert_eq!(got, b64);
    }

    #[test]
    fn armor_no_trailing_newline() {
        let b64 = "dGVzdA==";
        let text = format!("{ARMOR_BEGIN}\n{b64}\n{ARMOR_END}");
        assert!(parse_armor(&text).is_ok());
    }

    #[test]
    fn armor_rejects_crlf() {
        let text = format!("{ARMOR_BEGIN}\r\ndGVzdA==\r\n{ARMOR_END}\r\n");
        assert!(parse_armor(&text).is_err());
    }

    #[test]
    fn armor_rejects_wrong_begin() {
        let text = format!("-----BEGIN SOMETHING-----\ndGVzdA==\n{ARMOR_END}\n");
        assert!(parse_armor(&text).is_err());
    }

    #[test]
    fn armor_rejects_wrong_end() {
        let text = format!("{ARMOR_BEGIN}\ndGVzdA==\n-----END SOMETHING-----\n");
        assert!(parse_armor(&text).is_err());
    }

    #[test]
    fn armor_rejects_trailing_whitespace_on_b64() {
        let text = format!("{ARMOR_BEGIN}\ndGVzdA==  \n{ARMOR_END}\n");
        assert!(parse_armor(&text).is_err());
    }

    #[test]
    fn armor_rejects_oversized_b64() {
        let big = "A".repeat(MAX_B64_LINE + 1);
        let text = format!("{ARMOR_BEGIN}\n{big}\n{ARMOR_END}\n");
        assert!(parse_armor(&text).is_err());
    }

    // ── parse_envelope ────────────────────────────────────────────────────────

    fn valid_pk() -> String {
        // A known valid BIP-340 x-only pubkey (32 bytes, on-curve).
        "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".to_string()
    }

    fn valid_sig() -> String {
        "a".repeat(128)
    }

    fn valid_envelope_json() -> String {
        format!(
            r#"{{"v":1,"pk":"{pk}","sig":"{sig}","t":1700000000}}"#,
            pk = valid_pk(),
            sig = valid_sig(),
        )
    }

    #[test]
    fn envelope_valid_minimal() {
        let env = parse_envelope(&valid_envelope_json()).unwrap();
        assert_eq!(env.pk, valid_pk());
        assert_eq!(env.t, 1700000000);
        assert!(env.oa.is_none());
    }

    #[test]
    fn envelope_rejects_missing_v() {
        let json = format!(
            r#"{{"pk":"{pk}","sig":"{sig}","t":1700000000}}"#,
            pk = valid_pk(),
            sig = valid_sig(),
        );
        assert!(parse_envelope(&json).is_err());
    }

    #[test]
    fn envelope_rejects_v_not_1() {
        let json = format!(
            r#"{{"v":2,"pk":"{pk}","sig":"{sig}","t":1700000000}}"#,
            pk = valid_pk(),
            sig = valid_sig(),
        );
        assert!(parse_envelope(&json).is_err());
    }

    #[test]
    fn envelope_rejects_unknown_field() {
        let json = format!(
            r#"{{"v":1,"pk":"{pk}","sig":"{sig}","t":1700000000,"extra":"bad"}}"#,
            pk = valid_pk(),
            sig = valid_sig(),
        );
        assert!(parse_envelope(&json).is_err());
    }

    #[test]
    fn envelope_rejects_uppercase_pk() {
        let pk_upper = valid_pk().to_uppercase();
        let json = format!(
            r#"{{"v":1,"pk":"{pk_upper}","sig":"{sig}","t":1700000000}}"#,
            sig = valid_sig(),
        );
        assert!(parse_envelope(&json).is_err());
    }

    #[test]
    fn envelope_rejects_t_as_float() {
        let json = format!(
            r#"{{"v":1,"pk":"{pk}","sig":"{sig}","t":1700000000.0}}"#,
            pk = valid_pk(),
            sig = valid_sig(),
        );
        assert!(parse_envelope(&json).is_err());
    }

    #[test]
    fn envelope_rejects_t_out_of_range() {
        let json = format!(
            r#"{{"v":1,"pk":"{pk}","sig":"{sig}","t":9999999999}}"#,
            pk = valid_pk(),
            sig = valid_sig(),
        );
        assert!(parse_envelope(&json).is_err());
    }

    #[test]
    fn envelope_rejects_sig_wrong_length() {
        let json = format!(
            r#"{{"v":1,"pk":"{pk}","sig":"aabbcc","t":1700000000}}"#,
            pk = valid_pk(),
        );
        assert!(parse_envelope(&json).is_err());
    }

    // ── build_canonical_json ──────────────────────────────────────────────────

    #[test]
    fn canonical_json_no_oa() {
        let env = Envelope {
            pk: valid_pk(),
            sig: valid_sig(),
            t: 1700000000,
            oa: None,
        };
        let got = build_canonical_json(&env);
        let expected = format!(
            r#"{{"v":1,"pk":"{pk}","sig":"{sig}","t":1700000000}}"#,
            pk = valid_pk(),
            sig = valid_sig(),
        );
        assert_eq!(got, expected);
    }

    #[test]
    fn canonical_json_roundtrip() {
        let json = valid_envelope_json();
        let env = parse_envelope(&json).unwrap();
        assert_eq!(build_canonical_json(&env).as_bytes(), json.as_bytes());
    }

    // ── format_date ───────────────────────────────────────────────────────────

    #[test]
    fn format_date_epoch() {
        assert_eq!(format_date(0), "1970-01-01");
    }

    #[test]
    fn format_date_known() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        assert_eq!(format_date(1704067200), "2024-01-01");
    }

    #[test]
    fn format_date_leap_day() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        assert_eq!(format_date(1709164800), "2024-02-29");
    }

    // ── parse_oa_tag ──────────────────────────────────────────────────────────

    #[test]
    fn oa_tag_rejects_invalid_owner_hex() {
        let json = r#"["auth","ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ","read","aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]"#;
        assert!(parse_oa_tag(json).is_err());
    }

    #[test]
    fn oa_tag_rejects_invalid_sig_hex() {
        let json = format!(r#"["auth","{}","read","ZZZZ"]"#, valid_pk());
        assert!(parse_oa_tag(&json).is_err());
    }

    #[test]
    fn oa_tag_rejects_dangerous_conditions() {
        let json = format!(
            r#"["auth","{}","read; rm -rf /","{}"]"#,
            valid_pk(),
            valid_sig()
        );
        assert!(parse_oa_tag(&json).is_err());
    }

    #[test]
    fn oa_tag_rejects_wrong_label() {
        let json = format!(r#"["bad","{}","read","{}"]"#, valid_pk(), valid_sig());
        assert!(parse_oa_tag(&json).is_err());
    }

    // ── signing_message ───────────────────────────────────────────────────────

    #[test]
    fn signing_message_deterministic() {
        let payload = b"hello world";
        let m1 = signing_message(1700000000, None, payload);
        let m2 = signing_message(1700000000, None, payload);
        assert_eq!(m1, m2);
    }

    #[test]
    fn signing_message_differs_by_timestamp() {
        let payload = b"hello";
        let m1 = signing_message(1, None, payload);
        let m2 = signing_message(2, None, payload);
        assert_ne!(m1, m2);
    }

    #[test]
    fn signing_message_differs_with_oa() {
        let payload = b"hello";
        let oa = [valid_pk(), "read".to_string(), valid_sig()];
        let m_no_oa = signing_message(1, None, payload);
        let m_with_oa = signing_message(1, Some(&oa), payload);
        assert_ne!(m_no_oa, m_with_oa);
    }
}
