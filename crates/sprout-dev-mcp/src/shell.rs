use crate::shim::Shim;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const MAX_BYTES: usize = 50 * 1024;
const MAX_LINES: usize = 2000;
const TAIL_BYTES: usize = 8 * 1024;
const ARTIFACT_RING_SIZE: usize = 8;

pub struct SharedState {
    pub cwd: PathBuf,
    pub shim: Shim,
    pub session_dir: TempDir,
    pub bootstrap_instructions: String,
    pub artifacts: Mutex<VecDeque<PathBuf>>,
    pub todo_path: PathBuf,
    next_call_id: Mutex<u64>,
}

impl SharedState {
    pub fn new(cwd: PathBuf, shim: Shim) -> Self {
        let session_dir = tempfile::Builder::new()
            .prefix("sprout-dev-mcp-session-")
            .tempdir()
            .expect("create session tempdir");
        let todo_path = session_dir.path().join("todo.md");
        let bootstrap_instructions = build_bootstrap(&cwd);
        Self {
            cwd,
            shim,
            session_dir,
            bootstrap_instructions,
            artifacts: Mutex::new(VecDeque::with_capacity(ARTIFACT_RING_SIZE)),
            todo_path,
            next_call_id: Mutex::new(0),
        }
    }

    fn next_id(&self) -> u64 {
        let mut g = self.next_call_id.lock().expect("poisoned");
        *g += 1;
        *g
    }
}

fn build_bootstrap(cwd: &Path) -> String {
    let stack = detect_stack(cwd);
    format!(
        "sprout-dev-mcp — minimal dev tools for coding agents.\n\
         \n\
         Working directory: {}\n\
         Detected stack: {}\n\
         \n\
         Tools:\n\
         - shell(command, workdir?, timeout_ms?): run a bash command. Output is tail-truncated to ~8KB; full output goes to an artifact file.\n\
         - todo(content?): replace the TODO when content is given; read it when omitted.\n\
         - str_replace(path, old_str, new_str, workdir?): atomic find-and-replace. `old_str` must occur exactly once. Returns a unified diff.\n\
         \n\
         On PATH inside shell:\n\
         - rg: ripgrep-compatible search. Flags: -n, -i, -l, -g <glob>, -C <n>, --files. Falls back to a built-in implementation if system ripgrep is missing.\n\
         \n\
         Conventions: prefer str_replace over sed/awk for edits. Use `rg` instead of grep -r. Pass `workdir` per call rather than `cd`.\n",
        cwd.display(),
        stack,
    )
}

fn detect_stack(cwd: &Path) -> String {
    let markers = [
        ("Cargo.toml", "rust (cargo)"),
        ("package.json", "node"),
        ("go.mod", "go"),
        ("pyproject.toml", "python (pyproject)"),
        ("requirements.txt", "python"),
        ("Gemfile", "ruby"),
        ("pom.xml", "java (maven)"),
        ("build.gradle", "java (gradle)"),
        ("build.gradle.kts", "kotlin (gradle)"),
    ];
    let mut found: Vec<&str> = markers
        .iter()
        .filter(|(f, _)| cwd.join(f).exists())
        .map(|(_, name)| *name)
        .collect();
    if found.is_empty() {
        "unknown".into()
    } else {
        found.sort();
        found.join(", ")
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellParams {
    pub command: String,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

pub async fn run(state: &SharedState, p: ShellParams) -> String {
    let timeout_ms = p.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let workdir: PathBuf = p
        .workdir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.cwd.clone());

    if !workdir.is_dir() {
        return json_response(ShellResult {
            exit_code: -1,
            stdout: String::new(),
            stderr: format!(
                "workdir does not exist or is not a directory: {}\n",
                workdir.display()
            ),
            timed_out: false,
            duration_ms: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            stdout_artifact: None,
            stderr_artifact: None,
        });
    }

    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(&p.command);
    cmd.current_dir(&workdir);
    cmd.env("PATH", &state.shim.path_env);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);
    set_process_group(&mut cmd);

    let started = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return json_response(ShellResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("failed to spawn bash: {e}\n"),
                timed_out: false,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout_truncated: false,
                stderr_truncated: false,
                stdout_artifact: None,
                stderr_artifact: None,
            });
        }
    };

    let pid = child.id();
    let mut stdout_pipe = child.stdout.take().expect("piped");
    let mut stderr_pipe = child.stderr.take().expect("piped");

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let read_stdout = stdout_pipe.read_to_end(&mut stdout_buf);
    let read_stderr = stderr_pipe.read_to_end(&mut stderr_buf);

    let wait_fut = async {
        let _ = tokio::join!(read_stdout, read_stderr);
        child.wait().await
    };

    let timeout = Duration::from_millis(timeout_ms);
    let (status, timed_out) = match tokio::time::timeout(timeout, wait_fut).await {
        Ok(Ok(s)) => (Some(s), false),
        Ok(Err(_)) => (None, false),
        Err(_) => {
            if let Some(pid) = pid {
                kill_process_group(pid as i32);
            }
            (None, true)
        }
    };

    let duration_ms = started.elapsed().as_millis() as u64;
    let exit_code = status
        .as_ref()
        .and_then(|s| s.code())
        .unwrap_or(if timed_out { 124 } else { -1 });

    let id = state.next_id();
    let (stdout_text, stdout_truncated, stdout_artifact) =
        finalize_stream(state, id, "stdout", stdout_buf);
    let (stderr_text, stderr_truncated, stderr_artifact) =
        finalize_stream(state, id, "stderr", stderr_buf);

    json_response(ShellResult {
        exit_code,
        stdout: stdout_text,
        stderr: stderr_text,
        timed_out,
        duration_ms,
        stdout_truncated,
        stderr_truncated,
        stdout_artifact,
        stderr_artifact,
    })
}

#[cfg(unix)]
fn set_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn set_process_group(_cmd: &mut Command) {}

#[cfg(unix)]
fn kill_process_group(pid: i32) {
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;
    let pgid = Pid::from_raw(pid);
    let _ = killpg(pgid, Signal::SIGTERM);
    std::thread::sleep(Duration::from_millis(200));
    let _ = killpg(pgid, Signal::SIGKILL);
}

#[cfg(not(unix))]
fn kill_process_group(_pid: i32) {}

struct ShellResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
    duration_ms: u64,
    stdout_truncated: bool,
    stderr_truncated: bool,
    stdout_artifact: Option<String>,
    stderr_artifact: Option<String>,
}

fn json_response(r: ShellResult) -> String {
    let v = serde_json::json!({
        "exit_code": r.exit_code,
        "stdout": r.stdout,
        "stderr": r.stderr,
        "timed_out": r.timed_out,
        "duration_ms": r.duration_ms,
        "stdout_truncated": r.stdout_truncated,
        "stderr_truncated": r.stderr_truncated,
        "stdout_artifact": r.stdout_artifact,
        "stderr_artifact": r.stderr_artifact,
    });
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())
}

fn finalize_stream(
    state: &SharedState,
    call_id: u64,
    label: &str,
    buf: Vec<u8>,
) -> (String, bool, Option<String>) {
    let raw_len = buf.len();
    let line_count = buf.iter().filter(|b| **b == b'\n').count();
    let needs_truncate = raw_len > MAX_BYTES || line_count > MAX_LINES;

    if !needs_truncate {
        return (lossy(buf), false, None);
    }

    let artifact_path = crate::shim::artifact_dir(state.session_dir.path())
        .join(format!("{call_id:06}.{label}.txt"));
    let _ = std::fs::write(&artifact_path, &buf);
    rotate_artifacts(state, artifact_path.clone());

    let tail_start = raw_len.saturating_sub(TAIL_BYTES);
    let tail_aligned = align_to_char_boundary(&buf, tail_start);
    let tail = lossy(buf[tail_aligned..].to_vec());

    let notice = format!(
        "[truncated: showing last {} bytes; {} bytes / {} lines total; full output at {}]\n",
        tail.len(),
        raw_len,
        line_count,
        artifact_path.display(),
    );
    let mut out = String::with_capacity(notice.len() + tail.len());
    out.push_str(&notice);
    out.push_str(&tail);
    (
        out,
        true,
        Some(artifact_path.to_string_lossy().into_owned()),
    )
}

fn align_to_char_boundary(buf: &[u8], start: usize) -> usize {
    let mut i = start.min(buf.len());
    while i < buf.len() && (buf[i] & 0xC0) == 0x80 {
        i += 1;
    }
    i
}

fn lossy(buf: Vec<u8>) -> String {
    String::from_utf8(buf).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

fn rotate_artifacts(state: &SharedState, new_path: PathBuf) {
    let mut ring = state.artifacts.lock().expect("poisoned");
    ring.push_back(new_path);
    while ring.len() > ARTIFACT_RING_SIZE {
        if let Some(old) = ring.pop_front() {
            let _ = std::fs::remove_file(old);
        }
    }
}
