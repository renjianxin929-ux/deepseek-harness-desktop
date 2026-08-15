//! Harness Desktop V0.1 — Rust backend.
//!
//! Responsibilities:
//!   * resolve the Node / dsh runtime the user actually uses
//!     (process PATH -> user's login shell -> common install locations)
//!   * hard version guard: only DeepSeek Harness 0.1.0-rc.6 may be started
//!     (never a silent upgrade or downgrade)
//!   * start `dsh web --host 127.0.0.1 --port 0` with cwd = user HOME and
//!     DSH_HOME untouched (the existing ~/.dsh is reused as-is)
//!   * wait until the harness is really ready (stdout port line + HTTP 200)
//!   * navigate the WebView to http://127.0.0.1:<port> (the official Harness Web UI)
//!   * manage the process lifecycle: graceful SIGTERM to the process group,
//!     then SIGKILL after a short timeout; never touches processes we did not start.

use std::collections::{HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, Url};

mod appearance;
mod css_scope;
mod session_guard;
use appearance::AppearanceState;

const REQUIRED_VERSION: &str = "0.1.0-rc.6";
const DSH_PACKAGE: &str = "@deepseek-ai/dsh";
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const PORT_LINE_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const LOG_TAIL_CAP: usize = 200;

static STARTING: AtomicBool = AtomicBool::new(false);
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Status model (serialized to the loading page)
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub version: String,
    pub runtime_path: String,
    pub port: Option<u16>,
    pub reason: String,
    pub stderr_tail: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub phase: String, // "runtime" | "starting" | "waiting" | "ready" | "error"
    pub message: String,
    pub detail: Option<String>,
    pub url: Option<String>,
    pub error: Option<ErrorInfo>,
}

impl StatusPayload {
    fn phase(phase: &str, message: impl Into<String>) -> Self {
        Self {
            phase: phase.into(),
            message: message.into(),
            detail: None,
            url: None,
            error: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime resolution
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub enum Invocation {
    /// `node <bin_js> web ...`
    Direct { bin_js: PathBuf },
    /// `node <npx> -y @deepseek-ai/dsh@0.1.0-rc.6 web ...`
    Npx { npx: PathBuf },
}

#[derive(Clone)]
pub struct Runtime {
    pub node: PathBuf,
    pub invocation: Invocation,
    pub source: String,
}

pub struct Resolved {
    pub runtime: Runtime,
    pub shell_path: String,
}

/// A collected candidate executable.
///
/// `name` is the *logical* command identity (`node`, `npm`, `npx`, `dsh`) and
/// `path` is the verified executable path, which may be canonicalized (i.e. a
/// symlink resolved to its target). The logical identity must never be derived
/// from `path.file_name()`, because canonicalizing a symlink such as
/// `npx -> npx-cli.js` changes the basename away from the command name.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BinCandidate {
    name: String,
    path: PathBuf,
}

fn home_dir() -> PathBuf {
    if let Ok(h) = env::var("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    if let Ok(out) = Command::new("/usr/bin/id").arg("-un").output() {
        let user = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !user.is_empty() {
            if let Ok(text) = fs::read_to_string("/etc/passwd") {
                for line in text.lines() {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 6 && parts[0] == user {
                        return PathBuf::from(parts[5]);
                    }
                }
            }
        }
    }
    PathBuf::from("/")
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<(i32, String, String), String> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.code().unwrap_or(-1),
            Ok(None) => {}
            Err(e) => return Err(format!("wait failed: {e}")),
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("command timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = so.read_to_string(&mut out);
    }
    if let Some(mut se) = child.stderr.take() {
        let _ = se.read_to_string(&mut err);
    }
    Ok((code, out, err))
}

fn canonical(p: &Path) -> Option<PathBuf> {
    fs::canonicalize(p).ok()
}

fn default_shell() -> PathBuf {
    env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"))
}

/// Ask the user's login shell for its own PATH and the locations of
/// node/npm/npx/dsh as the user would see them from a Terminal.
///
/// Each resolved command is reported as `HD_BIN\t<name>\t<path>` so the
/// logical command identity survives the later canonicalization of the path.
fn shell_probe() -> (String, Vec<BinCandidate>) {
    let shell = default_shell();
    let probe = "echo HD_PATH_BEGIN; printf '%s' \"$PATH\"; echo; echo HD_PATH_END; for b in node npm npx dsh; do p=$(command -v \"$b\" 2>/dev/null) && printf 'HD_BIN\\t%s\\t%s\\n' \"$b\" \"$p\"; done";
    let base = "/usr/bin:/bin:/usr/sbin:/sbin";
    let merged = match env::var("PATH") {
        Ok(p) if !p.is_empty() => format!("{base}:{p}"),
        _ => base.to_string(),
    };
    let mut cmd = Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(probe);
    cmd.env("PATH", &merged);
    let Ok((_, out, _)) = run_with_timeout(&mut cmd, Duration::from_secs(20)) else {
        return (merged, Vec::new());
    };
    let mut shell_path: Option<String> = None;
    let mut bins: Vec<BinCandidate> = Vec::new();
    let mut capture = false;
    for line in out.lines() {
        let line = line.trim();
        if line == "HD_PATH_BEGIN" {
            capture = true;
            continue;
        }
        if line == "HD_PATH_END" {
            capture = false;
            continue;
        }
        if capture {
            shell_path = Some(line.to_string());
            continue;
        }
        let Some(rest) = line.strip_prefix("HD_BIN\t") else {
            continue;
        };
        let mut parts = rest.splitn(2, '\t');
        let (Some(name), Some(p)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.is_empty() || p.is_empty() {
            continue;
        }
        if let Some(c) = canonical(Path::new(p)) {
            bins.push(BinCandidate {
                name: name.to_string(),
                path: c,
            });
        }
    }
    (shell_path.filter(|s| !s.is_empty()).unwrap_or(merged), bins)
}

fn push_unique(pool: &mut Vec<BinCandidate>, seen: &mut HashSet<PathBuf>, c: BinCandidate) {
    if seen.insert(c.path.clone()) {
        pool.push(c);
    }
}

/// Scan `dirs` for the four logical commands, canonicalizing each resolved
/// executable path while preserving its logical command name. Shared by
/// `pool_from_env` and the resolver regression tests so both exercise the same
/// candidate-collection path.
fn scan_bin_dirs(pool: &mut Vec<BinCandidate>, seen: &mut HashSet<PathBuf>, dirs: &[PathBuf]) {
    for dir in dirs {
        for name in ["node", "npm", "npx", "dsh"] {
            if let Some(c) = canonical(&dir.join(name)) {
                push_unique(
                    pool,
                    seen,
                    BinCandidate {
                        name: name.to_string(),
                        path: c,
                    },
                );
            }
        }
    }
}

fn pool_from_env(shell_bins: Vec<BinCandidate>) -> Vec<BinCandidate> {
    let mut pool: Vec<BinCandidate> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for b in shell_bins {
        push_unique(&mut pool, &mut seen, b);
    }
    if let Ok(p) = env::var("PATH") {
        let dirs: Vec<PathBuf> = p
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .collect();
        scan_bin_dirs(&mut pool, &mut seen, &dirs);
    }
    scan_bin_dirs(
        &mut pool,
        &mut seen,
        &[
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/opt/homebrew/bin"),
        ],
    );
    let home = home_dir();
    let mut vm_dirs: Vec<PathBuf> = Vec::new();
    for base in [
        home.join(".nvm/versions/node"),
        home.join(".volta/bin"),
        home.join(".fnm"),
        home.join(".local/share/mise/shims"),
        home.join(".bun/bin"),
    ] {
        if let Ok(entries) = fs::read_dir(&base) {
            for e in entries.flatten() {
                vm_dirs.push(if e.path().join("bin").is_dir() {
                    e.path().join("bin")
                } else {
                    e.path()
                });
            }
        }
    }
    scan_bin_dirs(&mut pool, &mut seen, &vm_dirs);
    pool
}

fn read_dsh_version(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if v.get("name").and_then(|n| n.as_str()) != Some(DSH_PACKAGE) {
        return None;
    }
    v.get("version")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}

fn npx_cache_dsh_dirs() -> Vec<PathBuf> {
    let npx_root = home_dir().join(".npm/_npx");
    let Ok(entries) = fs::read_dir(&npx_root) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for e in entries.flatten() {
        let d = e.path().join("node_modules").join(DSH_PACKAGE);
        if d.join("lib/bin.js").is_file() {
            dirs.push(d);
        }
    }
    dirs
}

fn find_bin<'a>(pool: &'a [BinCandidate], name: &str) -> Option<&'a BinCandidate> {
    pool.iter().find(|c| c.name == name)
}

fn pick_node(pool: &[BinCandidate]) -> Result<PathBuf, String> {
    let mut why = String::new();
    for cand in pool.iter().filter(|c| c.name == "node") {
        let mut cmd = Command::new(&cand.path);
        cmd.arg("--version");
        match run_with_timeout(&mut cmd, Duration::from_secs(10)) {
            Ok((_, out, _)) if !out.trim().is_empty() => return Ok(cand.path.clone()),
            Ok((_, _, _)) => why = format!("node at {} produced no output", cand.path.display()),
            Err(e) => why = format!("node at {} failed: {e}", cand.path.display()),
        }
    }
    Err(format!(
        "No working node binary found. Candidates checked: {}{}",
        pool.iter()
            .map(|c| c.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        if why.is_empty() {
            String::new()
        } else {
            format!(" (last error: {why})")
        }
    ))
}

fn probe_npx_version(node: &Path, npx: &Path) -> Result<String, String> {
    let mut cmd = Command::new(node);
    cmd.arg(npx)
        .arg("-y")
        .arg(format!("{DSH_PACKAGE}@{REQUIRED_VERSION}"))
        .arg("--version");
    cmd.env("HOME", home_dir());
    let (_, out, err) = run_with_timeout(&mut cmd, Duration::from_secs(90))?;
    let v = out.trim().to_string();
    if v.is_empty() {
        return Err(format!(
            "npx version probe produced no output: {}",
            err.trim()
        ));
    }
    Ok(v)
}

fn probe_version(rt: &Runtime) -> Result<String, String> {
    let mut cmd = Command::new(&rt.node);
    match &rt.invocation {
        Invocation::Direct { bin_js } => {
            cmd.arg(bin_js).arg("--version");
        }
        Invocation::Npx { npx } => {
            cmd.arg(npx)
                .arg("-y")
                .arg(format!("{DSH_PACKAGE}@{REQUIRED_VERSION}"))
                .arg("--version");
        }
    }
    cmd.env("HOME", home_dir());
    cmd.env("DSH_HOME", home_dir().join(".dsh"));
    let (_, out, err) = run_with_timeout(&mut cmd, Duration::from_secs(90))
        .map_err(|e| format!("version probe: {e}"))?;
    let v = out.trim().to_string();
    if v.is_empty() {
        return Err(format!(
            "version probe produced no output. stderr: {}",
            err.trim()
        ));
    }
    Ok(v)
}

/// Resolve the runtime. Priority:
///   1. test hooks (HD_FORCE_NODE / HD_FORCE_DSH_BIN)
///   2. `dsh` on PATH / login shell (must be @deepseek-ai/dsh, must be rc.6)
///   3. ~/.npm/_npx/*/node_modules/@deepseek-ai/dsh with version == rc.6
///   4. global npm root @deepseek-ai/dsh with version == rc.6
///   5. npx fallback pinned to @deepseek-ai/dsh@0.1.0-rc.6 (never a silent upgrade)
pub fn resolve_runtime() -> Result<Resolved, String> {
    let (shell_path, shell_bins) = shell_probe();
    let pool = pool_from_env(shell_bins);

    if let Ok(node_s) = env::var("HD_FORCE_NODE") {
        let node = PathBuf::from(&node_s);
        if !node.is_file() {
            return Err(format!(
                "HD_FORCE_NODE points to a non-existent node: {node_s}"
            ));
        }
        let dsh_bin = force_dsh_bin()?;
        return Ok(Resolved {
            runtime: Runtime {
                node,
                invocation: Invocation::Direct { bin_js: dsh_bin },
                source: "HD_FORCE_NODE test hook".into(),
            },
            shell_path,
        });
    }
    if let Ok(bin_s) = env::var("HD_FORCE_DSH_BIN") {
        let bin_js = PathBuf::from(&bin_s);
        if !bin_js.is_file() {
            return Err(format!(
                "HD_FORCE_DSH_BIN points to a non-existent dsh entry: {bin_s}"
            ));
        }
        let node = pick_node(&pool)?;
        return Ok(Resolved {
            runtime: Runtime {
                node,
                invocation: Invocation::Direct { bin_js },
                source: "HD_FORCE_DSH_BIN test hook".into(),
            },
            shell_path,
        });
    }

    // 1) dsh on PATH / login shell
    for cand in pool.iter().filter(|c| c.name == "dsh") {
        let mut dir = cand.path.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if d.join("package.json").is_file() {
                if let Some(ver) = read_dsh_version(&d) {
                    let bin_js = d.join("lib/bin.js");
                    if !bin_js.is_file() {
                        break;
                    }
                    if ver == REQUIRED_VERSION {
                        let node = pick_node(&pool)?;
                        return Ok(Resolved {
                            runtime: Runtime {
                                node,
                                invocation: Invocation::Direct { bin_js },
                                source: format!("dsh on PATH ({})", cand.path.display()),
                            },
                            shell_path,
                        });
                    }
                    return Err(format!(
                        "Found dsh at {} with version {ver}, expected {REQUIRED_VERSION}. Refusing to start: Harness Desktop never silently upgrades or downgrades Harness.",
                        cand.path.display()
                    ));
                }
                break;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // 2) npx cache scan
    let mut found_other: Vec<String> = Vec::new();
    for d in npx_cache_dsh_dirs() {
        if let Some(ver) = read_dsh_version(&d) {
            let bin_js = d.join("lib/bin.js");
            if ver == REQUIRED_VERSION && bin_js.is_file() {
                let node = pick_node(&pool)?;
                return Ok(Resolved {
                    runtime: Runtime {
                        node,
                        invocation: Invocation::Direct { bin_js },
                        source: "~/.npm/_npx cache (rc.6)".into(),
                    },
                    shell_path,
                });
            }
            found_other.push(format!("{} ({})", d.display(), ver));
        }
    }

    // 3) global npm root
    if let Some(npm) = find_bin(&pool, "npm") {
        let mut cmd = Command::new(&npm.path);
        cmd.arg("root").arg("-g");
        if let Ok((_, out, _)) = run_with_timeout(&mut cmd, Duration::from_secs(15)) {
            let root = PathBuf::from(out.trim());
            let d = root.join(DSH_PACKAGE);
            if let Some(ver) = read_dsh_version(&d) {
                let bin_js = d.join("lib/bin.js");
                if ver == REQUIRED_VERSION && bin_js.is_file() {
                    let node = pick_node(&pool)?;
                    return Ok(Resolved {
                        runtime: Runtime {
                            node,
                            invocation: Invocation::Direct { bin_js },
                            source: "global npm root (rc.6)".into(),
                        },
                        shell_path,
                    });
                }
                found_other.push(format!("{} ({})", d.display(), ver));
            }
        }
    }

    // 4) npx fallback pinned to rc.6
    let node = pick_node(&pool)?;
    let npx = find_bin(&pool, "npx")
        .map(|c| c.path.clone())
        .ok_or_else(|| {
            let extra = if found_other.is_empty() {
                String::new()
            } else {
                format!("\nHarness found but not rc.6: {}", found_other.join("; "))
            };
            format!(
                "No npx available to bootstrap {DSH_PACKAGE}; PATH={}{extra}",
                env::var("PATH").unwrap_or_default()
            )
        })?;
    let probe = probe_npx_version(&node, &npx)?;
    if probe != REQUIRED_VERSION {
        return Err(format!(
            "npx {DSH_PACKAGE} resolved to version {probe}, expected {REQUIRED_VERSION}. Refusing to start."
        ));
    }
    Ok(Resolved {
        runtime: Runtime {
            node,
            invocation: Invocation::Npx { npx },
            source: "npx pinned @deepseek-ai/dsh@0.1.0-rc.6".into(),
        },
        shell_path,
    })
}

fn force_dsh_bin() -> Result<PathBuf, String> {
    // used together with HD_FORCE_NODE; resolve a real rc.6 entry
    for d in npx_cache_dsh_dirs() {
        if let Some(ver) = read_dsh_version(&d) {
            let bin_js = d.join("lib/bin.js");
            if ver == REQUIRED_VERSION && bin_js.is_file() {
                return Ok(bin_js);
            }
        }
    }
    Err("HD_FORCE_NODE set but no rc.6 dsh entry found in ~/.npm/_npx".into())
}

// ---------------------------------------------------------------------------
// Process management
// ---------------------------------------------------------------------------

fn parse_port_line(line: &str) -> Option<u16> {
    let marker = "http://127.0.0.1:";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let port: u16 = digits.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(port)
}

fn http_ok(port: u16) -> bool {
    let addr = format!("127.0.0.1:{port}");
    let Ok(mut s) = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_millis(700))
    else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(700)));
    let req = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if s.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = Vec::new();
    if s.read_to_end(&mut buf).is_err() {
        return false;
    }
    let head = String::from_utf8_lossy(&buf);
    head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200")
}

fn tail_of(tail: &Arc<Mutex<VecDeque<String>>>) -> String {
    tail.lock()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn process_exited(state: &State<'_, AppState>) -> bool {
    let mut mgr = state.process.lock().unwrap();
    match mgr.as_mut() {
        Some(hp) => matches!(hp.child.try_wait(), Ok(Some(_))),
        None => true,
    }
}

fn log_line(line: &str) {
    let p = home_dir().join("Library/Logs/HarnessDesktop/startup.log");
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{ts}] {line}");
    }
}

fn kill_group_graceful(mut hp: HarnessProcess) {
    let pid = hp.child.id() as i32;
    if pid <= 1 {
        return;
    }
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let deadline = Instant::now() + SHUTDOWN_GRACE;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = hp.child.try_wait() {
            let _ = hp.child.wait();
            log_line(&format!(
                "[shutdown] harness process group {pid} exited gracefully"
            ));
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = hp.child.wait();
    log_line(&format!(
        "[shutdown] harness process group {pid} force-killed"
    ));
}

fn shutdown_process(state: &State<'_, AppState>) {
    let mut mgr = state.process.lock().unwrap();
    if let Some(hp) = mgr.take() {
        kill_group_graceful(hp);
    }
}

// ---------------------------------------------------------------------------
// Startup flow
// ---------------------------------------------------------------------------

fn set_status(app: &AppHandle, s: StatusPayload) {
    let phase = s.phase.clone();
    let err_reason = s.error.as_ref().map(|e| e.reason.clone());
    let state = app.state::<AppState>();
    *state.status.lock().unwrap() = s.clone();
    let _ = app.emit("harness-status", s);
    log_line(&format!("[status] {phase}"));
    if let Some(r) = err_reason {
        log_line(&format!("[error] {r}"));
    }
}

fn error_payload(
    reason: String,
    version: String,
    runtime_path: String,
    port: Option<u16>,
    stderr_tail: String,
) -> StatusPayload {
    StatusPayload {
        phase: "error".into(),
        message: "Harness failed to start".into(),
        detail: None,
        url: None,
        error: Some(ErrorInfo {
            version,
            runtime_path,
            port,
            reason,
            stderr_tail,
        }),
    }
}

fn start_flow(app: &AppHandle) {
    let state = app.state::<AppState>();
    // Make sure any previous instance we manage is gone before starting again.
    shutdown_process(&state);

    set_status(app, StatusPayload::phase("runtime", "Resolving runtime..."));

    let resolved = match resolve_runtime() {
        Ok(r) => r,
        Err(reason) => {
            set_status(
                app,
                error_payload(
                    reason,
                    "unknown".into(),
                    "not resolved".into(),
                    None,
                    String::new(),
                ),
            );
            return;
        }
    };

    // Hard version guard: only 0.1.0-rc.6 may be started.
    let version = match probe_version(&resolved.runtime) {
        Ok(v) => v,
        Err(e) => {
            set_status(
                app,
                error_payload(
                    format!("Version probe failed: {e}"),
                    "unknown".into(),
                    resolved.runtime.node.display().to_string(),
                    None,
                    String::new(),
                ),
            );
            return;
        }
    };
    if version != REQUIRED_VERSION {
        set_status(app, error_payload(
            format!(
                "Detected Harness version {version}, but Harness Desktop requires exactly {REQUIRED_VERSION}. Refusing to start: no silent upgrade or downgrade."
            ),
            version,
            resolved.runtime.node.display().to_string(),
            None,
            String::new(),
        ));
        return;
    }

    let runtime_path = resolved.runtime.node.display().to_string();
    let source = resolved.runtime.source.clone();

    set_status(
        app,
        StatusPayload::phase(
            "runtime",
            format!("Runtime found: {source} (Harness {version})"),
        ),
    );
    set_status(
        app,
        StatusPayload::phase("starting", "Starting DeepSeek Harness..."),
    );

    // Build: node <dsh entry> web --host 127.0.0.1 --port 0   (port 0 => OS picks a free port)
    let mut cmd = Command::new(&resolved.runtime.node);
    match &resolved.runtime.invocation {
        Invocation::Direct { bin_js } => {
            cmd.arg(bin_js);
        }
        Invocation::Npx { npx } => {
            cmd.arg(npx)
                .arg("-y")
                .arg(format!("{DSH_PACKAGE}@{REQUIRED_VERSION}"));
        }
    }
    cmd.arg("web")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0");

    let home = home_dir();
    cmd.current_dir(&home);
    let mut path = String::new();
    if let Some(dir) = resolved.runtime.node.parent() {
        path.push_str(&format!("{}:", dir.display()));
    }
    if !resolved.shell_path.is_empty() {
        path.push_str(&resolved.shell_path);
        path.push(':');
    }
    if let Ok(p) = env::var("PATH") {
        path.push_str(&p);
    }
    cmd.env("PATH", &path);
    cmd.env("HOME", &home);
    cmd.env("DSH_HOME", home.join(".dsh")); // explicit, but identical to the default: reuse existing ~/.dsh
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // own process group so we only ever kill our own tree
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            set_status(
                app,
                error_payload(
                    format!("Failed to spawn harness: {e}"),
                    version.clone(),
                    runtime_path.clone(),
                    None,
                    String::new(),
                ),
            );
            return;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let log_tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let (port_tx, port_rx) = mpsc::channel::<u16>();

    if let Some(out) = stdout {
        let tail = log_tail.clone();
        let tx = port_tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines().map_while(Result::ok) {
                {
                    let mut t = tail.lock().unwrap();
                    if t.len() >= LOG_TAIL_CAP {
                        t.pop_front();
                    }
                    t.push_back(format!("[out] {line}"));
                }
                if let Some(port) = parse_port_line(&line) {
                    let _ = tx.send(port);
                }
            }
        });
    }
    if let Some(err) = stderr {
        let tail = log_tail.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(err);
            for line in reader.lines().map_while(Result::ok) {
                let mut t = tail.lock().unwrap();
                if t.len() >= LOG_TAIL_CAP {
                    t.pop_front();
                }
                t.push_back(format!("[err] {line}"));
            }
        });
    }
    drop(port_tx);

    // The app owns this process from now on.
    {
        let mut mgr = state.process.lock().unwrap();
        *mgr = Some(HarnessProcess {
            child,
            port: 0,
            runtime_path: runtime_path.clone(),
            version: version.clone(),
            log_tail: log_tail.clone(),
        });
    }

    set_status(
        app,
        StatusPayload::phase("waiting", "Waiting for Harness..."),
    );

    // 1) wait for the authoritative port announcement on stdout
    let port = match port_rx.recv_timeout(PORT_LINE_TIMEOUT) {
        Ok(p) => p,
        Err(_) => {
            let tail = tail_of(&log_tail);
            let exited = process_exited(&state);
            shutdown_process(&state);
            let reason = if exited {
                "The Harness process exited before becoming ready.".into()
            } else {
                "Timed out waiting for the Harness port announcement.".into()
            };
            set_status(
                app,
                error_payload(reason, version, runtime_path, None, tail),
            );
            return;
        }
    };
    {
        let mut mgr = state.process.lock().unwrap();
        if let Some(hp) = mgr.as_mut() {
            hp.port = port;
        }
    }

    // 2) poll the HTTP endpoint until it really answers
    let deadline = Instant::now() + READY_TIMEOUT;
    let mut ready = false;
    while Instant::now() < deadline {
        if http_ok(port) {
            ready = true;
            break;
        }
        if process_exited(&state) {
            break;
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    if !ready {
        let tail = tail_of(&log_tail);
        let exited = process_exited(&state);
        shutdown_process(&state);
        let reason = if exited {
            "The Harness process exited before the readiness check passed.".into()
        } else {
            format!("Harness did not answer on http://127.0.0.1:{port} within 30 seconds.")
        };
        set_status(
            app,
            error_payload(reason, version, runtime_path, Some(port), tail),
        );
        return;
    }

    // 3) ready: navigate the WebView to the official Harness Web UI
    let url = format!("http://127.0.0.1:{port}");
    log_line(&format!("[ready] {url}"));
    set_status(
        app,
        StatusPayload {
            phase: "ready".into(),
            message: "Ready".into(),
            detail: Some(url.clone()),
            url: Some(url.clone()),
            error: None,
        },
    );

    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window("main") {
            if let Ok(u) = Url::parse(&url) {
                let _ = w.navigate(u);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tauri integration
// ---------------------------------------------------------------------------

pub struct HarnessProcess {
    pub child: Child,
    pub port: u16,
    pub runtime_path: String,
    pub version: String,
    pub log_tail: Arc<Mutex<VecDeque<String>>>,
}

pub struct AppState {
    pub status: Mutex<StatusPayload>,
    pub process: Mutex<Option<HarnessProcess>>,
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> StatusPayload {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
fn restart(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    shutdown_process(&state);
    spawn_start_flow(&app);
    Ok(())
}

struct FlowGuard;
impl Drop for FlowGuard {
    fn drop(&mut self) {
        STARTING.store(false, Ordering::SeqCst);
    }
}

fn spawn_start_flow(app: &AppHandle) {
    if STARTING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _g = FlowGuard;
        start_flow(&app);
    });
}

fn build_app_menu<R: tauri::Runtime, M: Manager<R>>(manager: &M) -> tauri::Result<Menu<R>> {
    let appearance_item = MenuItemBuilder::with_id("appearance", "Appearance…")
        .accelerator("CmdOrCtrl+,")
        .build(manager)?;
    let app_menu = SubmenuBuilder::new(manager, "Harness Desktop")
        .item(&PredefinedMenuItem::about(
            manager,
            Some("About Harness Desktop"),
            None,
        )?)
        .separator()
        .item(&appearance_item)
        .separator()
        .item(&PredefinedMenuItem::quit(
            manager,
            Some("Quit Harness Desktop"),
        )?)
        .build()?;
    // Replacing the app menu via `set_menu` removes Tauri's default macOS menu,
    // which includes the standard Edit menu. Without an Edit menu, the native
    // Cmd+C / Cmd+X / Cmd+V / Cmd+A key equivalents are no longer bound to the
    // WKWebView responder chain (right-click paste still works because the
    // WebView's own context menu is independent of the app menu). Restore the
    // standard Edit menu so native clipboard shortcuts keep working.
    let edit_menu = SubmenuBuilder::new(manager, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    MenuBuilder::new(manager)
        .item(&app_menu)
        .item(&edit_menu)
        .build()
}

pub fn run() {
    let app = tauri::Builder::default()
        // Wallpaper image is served to the main window only, from the local
        // wallpaper directory. The image never leaves the app.
        .register_uri_scheme_protocol("hd-wallpaper", |ctx, request| {
            let state = ctx.app_handle().state::<AppearanceState>();
            appearance::serve_wallpaper(&state, ctx.webview_label(), &request)
        })
        // Diagnostic beacon: the injected engine reports its compatibility
        // state (compatible/degraded/fallback) here so it can be observed in
        // the startup log. No sensitive data is transmitted.
        .register_uri_scheme_protocol("hd-beacon", |_ctx, request| {
            log_line(&format!("[appearance-beacon] {}", request.uri()));
            tauri::http::Response::builder()
                .status(204)
                .body(Vec::new())
                .unwrap()
        })
        // Safety net for the injected appearance button: navigation is blocked
        // by the main window's `on_navigation` guard; this handler only runs if
        // that guard is bypassed, and it never navigates the page.
        .register_uri_scheme_protocol("hd-appearance", |ctx, _request| {
            appearance::open_appearance_window(ctx.app_handle());
            tauri::http::Response::builder()
                .status(204)
                .body(Vec::new())
                .unwrap()
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "appearance" {
                appearance::open_appearance_window(app);
            }
        })
        .setup(|app| {
            app.manage(AppState {
                status: Mutex::new(StatusPayload::phase("runtime", "Starting...")),
                process: Mutex::new(None),
            });

            // Appearance state + the injected engine script. Init is infallible
            // so an appearance problem can never stop Harness from launching.
            let handle = app.handle().clone();
            let appearance_state = AppearanceState::init(&handle);

            // Test/automation hook: install a wallpaper from a local path at
            // startup (never part of the interactive user flow). Runs BEFORE
            // the engine script is built so the initial payload is correct.
            if let Ok(p) = env::var("HD_FORCE_WALLPAPER") {
                let _ = appearance::install_wallpaper_from_path(
                    &handle,
                    &appearance_state,
                    Path::new(&p),
                );
            }

            // Appearance engine + the stale session guard. Both are plain
            // injected page scripts; the guard has no Tauri IPC and only ever
            // performs a read-only session.list query plus, at most, a reload.
            let init_script = format!(
                "{}\n{}",
                appearance::build_init_script(&appearance_state),
                session_guard::SCRIPT
            );
            app.manage(appearance_state);

            // Create the main window programmatically so we can attach the
            // initialization script and the navigation guard.
            let nav_handle = handle.clone();
            let _main = tauri::WebviewWindowBuilder::new(
                &handle,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Harness Desktop")
            .inner_size(1280.0, 800.0)
            .min_inner_size(960.0, 600.0)
            .center()
            .resizable(true)
            .initialization_script(init_script)
            .on_navigation(move |url| {
                if url.scheme() == "hd-appearance" {
                    appearance::open_appearance_window(&nav_handle);
                    return false;
                }
                true
            })
            .build()?;

            // Test/automation hook: open the appearance settings window at
            // startup.
            if env::var("HD_OPEN_APPEARANCE").is_ok() {
                appearance::open_appearance_window(&handle);
            }

            let _ = app.set_menu(build_app_menu(app.handle())?);

            // Watch termination signals so the harness we started is never
            // orphaned even when the app is killed by SIGTERM/SIGINT
            // (logout, system shutdown, `kill`, Activity Monitor, ...).
            let signal_handle = handle.clone();
            std::thread::spawn(move || {
                use signal_hook::consts::{SIGINT, SIGTERM};
                if let Ok(mut sigs) = signal_hook::iterator::Signals::new([SIGTERM, SIGINT]) {
                    // Handle the first termination signal: shut down the harness
                    // we started, then exit (only one signal needs handling).
                    if sigs.forever().next().is_some() {
                        if !SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
                            let state = signal_handle.state::<AppState>();
                            shutdown_process(&state);
                        }
                        std::process::exit(0);
                    }
                }
            });
            spawn_start_flow(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            restart,
            appearance::get_appearance,
            appearance::set_theme,
            appearance::set_motion,
            appearance::set_language,
            appearance::set_wallpaper,
            appearance::set_wallpaper_bytes,
            appearance::remove_wallpaper,
            appearance::reset_appearance
        ])
        .build(tauri::generate_context!())
        .expect("error while building Harness Desktop");

    app.run(|app_handle, event| match event {
        // Window close (last window destroyed) and app.exit() go through here.
        RunEvent::ExitRequested { api, .. } => {
            if !SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
                api.prevent_exit();
                let state = app_handle.state::<AppState>();
                shutdown_process(&state);
                app_handle.exit(0);
            }
        }
        // macOS Cmd+Q / quit AppleEvent terminate the app without
        // ExitRequested; tao maps applicationWillTerminate to RunEvent::Exit,
        // so cleanup runs here synchronously before the process dies.
        RunEvent::Exit if !SHUTTING_DOWN.swap(true, Ordering::SeqCst) => {
            let state = app_handle.state::<AppState>();
            shutdown_process(&state);
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = DIR_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
        let dir = env::temp_dir().join(format!(
            "hd-resolver-{tag}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Regression: a real macOS npm/npx install is a symlink to a `.js` target
    /// (`npx -> npx-cli.js`, `npm -> npm-cli.js`). Canonicalizing the path must
    /// not be allowed to change the logical command identity, so `find_bin`
    /// still finds `npx`/`npm` by name while `node -> node` keeps passing.
    #[test]
    fn symlinked_npm_npx_keep_logical_identity_after_canonicalization() {
        let dir = temp_dir("symlinks");
        fs::write(dir.join("npx-cli.js"), "#!/usr/bin/env node\n").unwrap();
        fs::write(dir.join("npm-cli.js"), "#!/usr/bin/env node\n").unwrap();
        fs::write(dir.join("node"), "#!/bin/sh\necho v18\n").unwrap();
        symlink(dir.join("npx-cli.js"), dir.join("npx")).unwrap();
        symlink(dir.join("npm-cli.js"), dir.join("npm")).unwrap();

        // Exactly the candidate-collection path `pool_from_env` uses per dir.
        let mut pool: Vec<BinCandidate> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        scan_bin_dirs(&mut pool, &mut seen, &[dir.clone()]);

        let npx = find_bin(&pool, "npx").expect("npx must be found");
        assert_eq!(npx.name, "npx");
        assert_eq!(npx.path, fs::canonicalize(dir.join("npx-cli.js")).unwrap());
        assert_eq!(npx.path.file_name().unwrap(), "npx-cli.js");

        let npm = find_bin(&pool, "npm").expect("npm must be found");
        assert_eq!(npm.name, "npm");
        assert_eq!(npm.path, fs::canonicalize(dir.join("npm-cli.js")).unwrap());
        assert_eq!(npm.path.file_name().unwrap(), "npm-cli.js");

        let node = find_bin(&pool, "node").expect("node must be found");
        assert_eq!(node.name, "node");
        assert_eq!(node.path, fs::canonicalize(dir.join("node")).unwrap());
        assert_eq!(node.path.file_name().unwrap(), "node");

        fs::remove_dir_all(&dir).ok();
    }

    /// The logical identity is decoupled from the canonical path's basename:
    /// `find_bin` matches `name`, never the resolved file name.
    #[test]
    fn find_bin_matches_logical_name_not_canonical_basename() {
        let pool = vec![BinCandidate {
            name: "npx".to_string(),
            path: PathBuf::from("/somewhere/else/npx-cli.js"),
        }];
        assert!(find_bin(&pool, "npx").is_some());
        assert!(find_bin(&pool, "npx-cli.js").is_none());
    }
}
