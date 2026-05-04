//! git-credential-nostr — NIP-98 git credential helper for Sprout.
//!
//! Git calls this binary via the credential helper protocol (stdin/stdout).
//! We read the request, sign a kind:27235 event, and return the base64-encoded
//! event as the credential value.  Git then sends:
//!   Authorization: Nostr <credential>

use std::io::{self, BufRead, Write};
use std::process;

use base64::Engine as _;
use nostr::nips::nip98::{HttpData, HttpMethod};
use nostr::{EventBuilder, Keys, UncheckedUrl};
use zeroize::Zeroize;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Write an error to stderr and exit 1.
/// Does NOT write to stdout — git's credential protocol interprets any stdout
/// as credential data, and a bare newline could confuse the client.
fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    process::exit(1);
}

/// Read `git config <key>` from the process environment / git config.
fn git_config(key: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["config", "--get", key])
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

/// Check that a file has permissions no broader than 0600.
/// On non-Unix platforms we warn and continue.
#[cfg(unix)]
fn check_keyfile_permissions(path: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat keyfile {path}: {e}"))?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o177 != 0 {
        return Err(format!(
            "keyfile {path} has insecure permissions (expected 0600)"
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_keyfile_permissions(path: &str) -> Result<(), String> {
    eprintln!("warning: cannot check keyfile permissions on this platform ({path})");
    Ok(())
}

/// Load the private key: env var first, then keyfile.
/// Returns the raw key string (nsec or hex).  Caller must zeroize after use.
fn load_key() -> Result<String, String> {
    // 1. Environment variable — ideal for CI/CD.
    if let Ok(val) = std::env::var("NOSTR_PRIVATE_KEY") {
        if !val.is_empty() {
            return Ok(val);
        }
    }

    // 2. keyfile path from git config.
    let path = git_config("nostr.keyfile").ok_or_else(|| {
        "no nostr key configured. Set $NOSTR_PRIVATE_KEY or git config nostr.keyfile".to_string()
    })?;

    check_keyfile_permissions(&path)?;

    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read keyfile {path}: {e}"))?;
    Ok(raw.trim().to_string())
}

// ── stdin parsing ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct CredRequest {
    has_authtype_capability: bool,
    protocol: Option<String>,
    host: Option<String>,
    path: Option<String>,
    wwwauth: Option<String>,
}

fn parse_stdin() -> CredRequest {
    let stdin = io::stdin();
    let mut req = CredRequest::default();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.is_empty() {
            break;
        }
        if line == "capability[]=authtype" {
            req.has_authtype_capability = true;
        } else if let Some(v) = line.strip_prefix("protocol=") {
            req.protocol = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("host=") {
            req.host = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("path=") {
            req.path = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("wwwauth[]=") {
            // Only capture Nostr challenges — ignore Basic, Bearer, etc.
            if v.starts_with("Nostr ") && req.wwwauth.is_none() {
                req.wwwauth = Some(v.to_string());
            }
        }
        // ignore unknown lines
    }

    req
}

/// Extract the HTTP method from a WWW-Authenticate value like:
///   Nostr realm="sprout", method="GET"
///
/// Splits on ", " first to isolate parameters — prevents matching inside
/// quoted values like `realm="evil method=\"DELETE\""`.
fn parse_method(wwwauth: &str) -> Option<HttpMethod> {
    for param in wwwauth.split(", ") {
        let param = param.trim();
        if let Some(rest) = param.strip_prefix("method=\"") {
            let end = rest.find('"')?;
            return rest[..end].parse().ok();
        }
    }
    None
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    // Git calls credential helpers with a subcommand: get, store, or erase.
    // We only handle "get" — store/erase are no-ops for ephemeral credentials.
    match std::env::args().nth(1).as_deref() {
        Some("get") | None => {} // proceed — None for backwards compat
        Some(_) => return,       // store, erase, or unknown → silent exit 0
    }

    let req = parse_stdin();

    // Old git without authtype capability — nothing we can do.
    // The blank line signals "no credential available" per git's protocol.
    if !req.has_authtype_capability {
        println!();
        let _ = io::stdout().flush();
        return;
    }

    // Validate required fields.
    let protocol = req
        .protocol
        .as_deref()
        .unwrap_or_else(|| fail("missing protocol in credential request"));
    let host = req
        .host
        .as_deref()
        .unwrap_or_else(|| fail("missing host in credential request"));
    let path = req
        .path
        .as_deref()
        .unwrap_or_else(|| fail("credential.useHttpPath must be true for NIP-98 auth"));

    let wwwauth = req
        .wwwauth
        .as_deref()
        .unwrap_or_else(|| fail("server did not include WWW-Authenticate header"));

    let method = parse_method(wwwauth)
        .unwrap_or_else(|| fail("server did not include method hint in WWW-Authenticate"));

    // Sign the repo root URL — strip endpoint suffixes to get the canonical form.
    //
    // Git's credential helper is invoked once (for the initial info/refs GET) and the
    // token is reused for subsequent requests (upload-pack, receive-pack POST). The
    // server verifies against the bare repo root URL.
    //
    // Git's credential protocol does NOT pass query strings in the `path` field, so
    // we never see `?service=...` here — just the path component.
    let repo_path = path
        .split_once("/info/refs")
        .map(|(prefix, _)| prefix)
        .or_else(|| path.strip_suffix("/git-upload-pack"))
        .or_else(|| path.strip_suffix("/git-receive-pack"))
        .unwrap_or(path);
    let url = format!("{protocol}://{host}/{repo_path}");

    // Load key, sign, then zeroize.
    let mut raw_key = match load_key() {
        Ok(k) => k,
        Err(e) => fail(&e),
    };

    let keys = match Keys::parse(&raw_key) {
        Ok(k) => k,
        Err(e) => {
            raw_key.zeroize();
            fail(&format!("invalid nostr private key: {e}"));
        }
    };
    raw_key.zeroize();

    let http_data = HttpData::new(UncheckedUrl::from(url.as_str()), method);
    let event = match EventBuilder::http_auth(http_data).sign_with_keys(&keys) {
        Ok(e) => e,
        Err(e) => fail(&format!("failed to sign NIP-98 event: {e}")),
    };

    let json = match serde_json::to_string(&event) {
        Ok(j) => j,
        Err(e) => fail(&format!("failed to serialize event: {e}")),
    };

    let credential = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());

    // Output the credential response.
    println!("capability[]=authtype");
    println!("authtype=Nostr");
    println!("credential={credential}");
    println!("ephemeral=true");
    println!("quit=true");
    println!();
    let _ = io::stdout().flush();
}
