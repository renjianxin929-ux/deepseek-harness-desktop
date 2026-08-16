//! DeepSeek Harness Desktop — cross-platform foundation.
//!
//! V0.2 introduces a minimal platform seam so the new runtime / reliability /
//! usage code does not deepen the macOS/Unix lock-in. This module is the ONLY
//! public cross-platform abstraction for the seams V0.2 actually needs:
//!
//!   * runtime target identity (`darwin-arm64`, `darwin-x64`, `windows-x64`)
//!   * home / user-profile directory resolution
//!   * executable naming (`node` vs `node.exe`)
//!   * owned process-tree lifecycle (`OwnedProcessTree`)
//!   * a small platform capability map
//!
//! It deliberately does NOT attempt a broad V0.1 platform rewrite. Existing
//! macOS-specific code that is unrelated to these seams remains in place; the
//! legacy system-runtime resolution in `lib.rs` is a dev-only path and is
//! documented as such, not moved here.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Runtime target identity
// ---------------------------------------------------------------------------

/// Identity of the platform a build runs on, used to select the bundled
/// runtime artifact in the resource layout. The `id` is a cross-platform
/// semantic name (NOT `UnixProcessGroup`-style OS leakage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetIdentity {
    pub id: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
}

impl TargetIdentity {
    pub fn current() -> Self {
        let os = if cfg!(target_os = "macos") {
            "darwin"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "unknown"
        };
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else if cfg!(target_arch = "x86_64") {
            "x64"
        } else {
            "unknown"
        };
        let id = match (os, arch) {
            ("darwin", "arm64") => "darwin-arm64",
            ("darwin", "x64") => "darwin-x64",
            ("windows", "x64") => "windows-x64",
            _ => "unknown",
        };
        Self { id, os, arch }
    }
}

// ---------------------------------------------------------------------------
// Home / user-profile resolution
// ---------------------------------------------------------------------------

/// Resolve the user's home directory for the current platform.
///
/// Unix keeps V0.1's behaviour (HOME, with a login-shell `/etc/passwd`
/// fallback) so launching from Finder/launchd without HOME keeps working.
/// Windows uses `USERPROFILE` (then `HOMEDRIVE`+`HOMEPATH`) and never touches
/// `/usr/bin/id` or `/etc/passwd`.
pub fn home_dir() -> PathBuf {
    #[cfg(unix)]
    {
        unix_home_dir()
    }
    #[cfg(windows)]
    {
        windows_home_dir()
    }
    #[cfg(not(any(unix, windows)))]
    {
        PathBuf::from("/")
    }
}

#[cfg(unix)]
fn unix_home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    // V0.1 fallback: ask `id` for the user name, then read `/etc/passwd`.
    if let Ok(out) = Command::new("/usr/bin/id").arg("-un").output() {
        let user = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !user.is_empty() {
            if let Ok(text) = std::fs::read_to_string("/etc/passwd") {
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

#[cfg(windows)]
fn windows_home_dir() -> PathBuf {
    if let Ok(p) = std::env::var("USERPROFILE") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(d) = std::env::var("HOMEDRIVE") {
        if let Ok(p) = std::env::var("HOMEPATH") {
            let joined = format!("{d}{p}");
            if !joined.is_empty() {
                return PathBuf::from(joined);
            }
        }
    }
    PathBuf::from("C:\\")
}

// ---------------------------------------------------------------------------
// Executable naming
// ---------------------------------------------------------------------------

/// Map a logical executable base name to the platform's file name
/// (`node` → `node.exe` on Windows). The logical identity is never derived from
/// a path basename; this mapping is used only when constructing filesystem
/// paths for the bundled runtime.
pub fn executable_name(base: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// The bundled Node executable name for the current platform.
pub fn node_name() -> String {
    executable_name("node")
}

/// The platform's PATH-list separator (`;` on Windows, `:` elsewhere).
pub fn path_separator() -> &'static str {
    if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    }
}

// ---------------------------------------------------------------------------
// Process-execution path normalization
// ---------------------------------------------------------------------------

/// Convert a Windows verbatim / extended-length path into an ordinary Win32
/// path. On Windows, `fs::canonicalize()` and Tauri's `resource_dir()` return
/// paths with a `\\?\` prefix:
///
/// ```text
/// \\?\E:\DeepSeek Harness Desktop\...\node.exe   (verbatim drive)
/// \\?\UNC\server\share\...\node.exe              (verbatim UNC)
/// ```
///
/// These verbatim forms remain valid for filesystem APIs (`exists`, `read`,
/// `canonicalize`, checksum), but Node.js's module resolver mishandles them
/// when they are used as the child process's main entry (`argv[1]`) or working
/// directory — it resolves the `E:` drive root as a directory and fails with
/// `EISDIR`. This helper converts the verbatim form to the ordinary Win32 form
/// at the *process-execution boundary only*:
///
/// ```text
/// \\?\E:\foo\bar            -> E:\foo\bar
/// \\?\UNC\server\share\foo  -> \\server\share\foo
/// ```
///
/// It is a pure string transform with no filesystem access, so it is safe to
/// unit test on any host. Anything that is not a verbatim path (an already
/// ordinary path, a plain UNC path, or a non-Windows path) is returned
/// unchanged, so it never corrupts UNC semantics or non-Windows paths.
// On non-Windows hosts the verbatim prefix never occurs, so this helper is
// only exercised by the unit tests there (and by `process_path` on Windows).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn strip_verbatim_prefix(input: &str) -> String {
    // Verbatim UNC: `\\?\UNC\server\share\...` -> `\\server\share\...`.
    // (Do NOT reduce this to a drive path; UNC semantics must be preserved.)
    if let Some(rest) = input.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    // Verbatim drive: `\\?\C:\...` -> `C:\...`.
    if let Some(rest) = input.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    input.to_string()
}

/// Normalize a path for hand-off to a spawned process. On Windows this strips
/// the verbatim prefix (see [`strip_verbatim_prefix`]); on every other platform
/// it is the identity. Filesystem / integrity resolution keeps the raw path —
/// only values handed to `Command` (executable, arguments, cwd, PATH) go
/// through here.
pub fn process_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(strip_verbatim_prefix(&path.to_string_lossy()))
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// The base app-origin URL Tauri serves embedded assets from (`tauri://localhost`
/// on macOS/Linux, `http://tauri.localhost` on Windows/Android).
pub fn app_origin_url() -> &'static str {
    if cfg!(windows) || cfg!(target_os = "android") {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    }
}

/// URL for an app-registered custom URI scheme, expressed for the current
/// platform. macOS/iOS register schemes as `<scheme>://<path>`; Windows/Android
/// resolve registered schemes as `http://<scheme>.localhost/<path>`.
pub fn custom_scheme_url(scheme: &str, path: &str) -> String {
    if cfg!(windows) || cfg!(target_os = "android") {
        format!("http://{scheme}.localhost/{path}")
    } else {
        format!("{scheme}://{path}")
    }
}

/// Platform-appropriate directory for Desktop host logs.
///
/// * macOS: `~/Library/Logs/HarnessDesktop` (unchanged V0.1/V0.2 path).
/// * Windows: `%LOCALAPPDATA%\HarnessDesktop\logs`, falling back to
///   `%USERPROFILE%\AppData\Local\HarnessDesktop\logs` (never a Unix path).
pub fn log_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(l) = std::env::var("LOCALAPPDATA") {
            if !l.is_empty() {
                return PathBuf::from(l).join("HarnessDesktop").join("logs");
            }
        }
        home_dir().join("AppData").join("Local").join("HarnessDesktop").join("logs")
    }
    #[cfg(unix)]
    {
        home_dir().join("Library/Logs/HarnessDesktop")
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::env::temp_dir().join("harness-desktop")
    }
}

// ---------------------------------------------------------------------------
// Platform capability map
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// Own process-group semantics (unix) vs. creation-flags-only (windows).
    pub process_group: bool,
    /// Sending a signal to a whole process group via a negative pid.
    pub signal_kill_group: bool,
    /// Specific-PID tree kill via `taskkill /PID <pid> /T` (windows).
    pub taskkill_tree: bool,
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        process_group: cfg!(unix),
        signal_kill_group: cfg!(unix),
        taskkill_tree: cfg!(windows),
    }
}

/// One-line diagnostic summary of the platform boundary (logged at startup).
pub fn describe() -> String {
    let t = TargetIdentity::current();
    let c = capabilities();
    format!(
        "target={} os={} arch={} node={} process_group={} signal_kill_group={} taskkill_tree={}",
        t.id,
        t.os,
        t.arch,
        node_name(),
        c.process_group,
        c.signal_kill_group,
        c.taskkill_tree
    )
}

// ---------------------------------------------------------------------------
// Owned process-tree lifecycle
// ---------------------------------------------------------------------------

/// How a graceful termination attempt ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminateOutcome {
    /// The owned child (and its group) exited within the grace period.
    Exited,
    /// The grace period elapsed and the tree had to be force-killed.
    ForceKilled,
    /// The tree was never owned (pid <= 1) so nothing was signalled.
    NotOwned,
}

/// A process tree Desktop spawned and therefore owns. Only this tree is ever
/// signalled; unrelated processes are never matched or touched.
pub struct OwnedProcessTree {
    child: Child,
    /// Unix process-group id (equals the leader pid after `process_group(0)`).
    #[cfg(unix)]
    group_id: i32,
}

impl OwnedProcessTree {
    /// Wrap an already-spawned `Child`. The caller must have configured the
    /// child for ownership via [`configure_spawn`] before spawning.
    pub fn wrap(child: Child) -> Self {
        #[cfg(unix)]
        let group_id = child.id() as i32;
        Self {
            child,
            #[cfg(unix)]
            group_id,
        }
    }

    /// Configure a `Command` so its child runs in its own owned group/tree.
    pub fn configure_spawn(cmd: &mut Command) {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Own process group: we can later signal exactly this group.
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait()
    }

    /// Ask the owned tree to terminate, escalating after `grace`. Never signals
    /// anything outside this tree.
    pub fn graceful_terminate(&mut self, grace: Duration) -> TerminateOutcome {
        #[cfg(unix)]
        {
            self.unix_graceful_terminate(grace)
        }
        #[cfg(windows)]
        {
            self.windows_graceful_terminate(grace)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = self.child.kill();
            let _ = self.child.wait();
            TerminateOutcome::ForceKilled
        }
    }

    /// Force-kill the owned tree. Used only as a last resort for an owned tree.
    pub fn kill_tree(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
impl OwnedProcessTree {
    fn unix_graceful_terminate(&mut self, grace: Duration) -> TerminateOutcome {
        let pid = self.group_id;
        if pid <= 1 {
            return TerminateOutcome::NotOwned;
        }
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if matches!(self.try_wait(), Ok(Some(_))) {
                let _ = self.wait();
                return TerminateOutcome::Exited;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
        let _ = self.wait();
        TerminateOutcome::ForceKilled
    }
}

#[cfg(windows)]
impl OwnedProcessTree {
    // Compile-checked only (WINDOWS_REAL_E2E=NOT_RUN). Windows has no portable
    // SIGTERM-equivalent for a console Node tree, so "graceful" degrades to a
    // targeted, specific-PID tree kill (never `taskkill /IM <name>`).
    fn windows_graceful_terminate(&mut self, grace: Duration) -> TerminateOutcome {
        let pid = self.child.id();
        if pid <= 1 {
            return TerminateOutcome::NotOwned;
        }
        // Give the child the grace window to exit on its own first.
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if matches!(self.try_wait(), Ok(Some(_))) {
                let _ = self.wait();
                return TerminateOutcome::Exited;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
        let _ = self.wait();
        TerminateOutcome::ForceKilled
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_identity_matches_compile_target() {
        let t = TargetIdentity::current();
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert_eq!(t.id, "darwin-arm64");
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        assert_eq!(t.id, "darwin-x64");
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        assert_eq!(t.id, "windows-x64");
        assert_eq!(t.os, if cfg!(target_os = "macos") { "darwin" } else if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "linux") { "linux" } else { "unknown" });
    }

    #[test]
    fn executable_name_is_platform_aware() {
        #[cfg(windows)]
        assert_eq!(executable_name("node"), "node.exe");
        #[cfg(not(windows))]
        assert_eq!(executable_name("node"), "node");
    }

    #[test]
    fn node_name_uses_executable_mapping() {
        #[cfg(windows)]
        assert_eq!(node_name(), "node.exe");
        #[cfg(not(windows))]
        assert_eq!(node_name(), "node");
    }

    #[test]
    fn path_separator_is_platform_aware() {
        #[cfg(windows)]
        assert_eq!(path_separator(), ";");
        #[cfg(not(windows))]
        assert_eq!(path_separator(), ":");
    }

    #[test]
    fn strip_verbatim_prefix_ordinary_path_unchanged() {
        // Ordinary drive path (already a normal Win32 path).
        let ordinary = r"C:\Program Files\DeepSeek Harness Desktop\node.exe";
        assert_eq!(strip_verbatim_prefix(ordinary), ordinary);
        // Plain UNC path (no verbatim prefix) is untouched.
        let unc = r"\\server\share\DeepSeek Harness Desktop\node.exe";
        assert_eq!(strip_verbatim_prefix(unc), unc);
        // Relative / bare file name is untouched.
        assert_eq!(strip_verbatim_prefix("node.exe"), "node.exe");
    }

    #[test]
    fn strip_verbatim_prefix_verbatim_drive() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\Program Files\DeepSeek Harness Desktop\node.exe"),
            r"C:\Program Files\DeepSeek Harness Desktop\node.exe"
        );
    }

    #[test]
    fn strip_verbatim_prefix_verbatim_unc() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share\DeepSeek Harness Desktop\node.exe"),
            r"\\server\share\DeepSeek Harness Desktop\node.exe"
        );
    }

    #[test]
    fn strip_verbatim_prefix_non_windows_unchanged() {
        // Non-Windows paths must never be altered.
        assert_eq!(strip_verbatim_prefix("/usr/local/bin/node"), "/usr/local/bin/node");
        assert_eq!(
            strip_verbatim_prefix("/Users/me/DeepSeek Harness Desktop/node"),
            "/Users/me/DeepSeek Harness Desktop/node"
        );
    }

    #[test]
    fn process_path_identity_and_boundary() {
        // On non-Windows hosts `process_path` must be the identity (the
        // verbatim prefix only ever appears on Windows).
        #[cfg(not(windows))]
        {
            let p = Path::new("/usr/local/bin/node");
            assert_eq!(process_path(p), p);
        }
        // On Windows it strips the verbatim prefix at the process boundary.
        #[cfg(windows)]
        {
            let verbatim = Path::new(r"\\?\E:\DeepSeek Harness Desktop\runtime\node.exe");
            assert_eq!(
                process_path(verbatim),
                PathBuf::from(r"E:\DeepSeek Harness Desktop\runtime\node.exe")
            );
            let verbatim_unc = Path::new(r"\\?\UNC\server\share\DeepSeek Harness Desktop\node.exe");
            assert_eq!(
                process_path(verbatim_unc),
                PathBuf::from(r"\\server\share\DeepSeek Harness Desktop\node.exe")
            );
        }
    }

    #[test]
    fn capabilities_are_consistent_with_cfg() {
        let c = capabilities();
        assert_eq!(c.process_group, cfg!(unix));
        assert_eq!(c.signal_kill_group, cfg!(unix));
        assert_eq!(c.taskkill_tree, cfg!(windows));
    }

    #[test]
    fn home_dir_is_absolute_and_non_empty() {
        let h = home_dir();
        assert!(!h.as_os_str().is_empty());
        assert!(h.is_absolute());
    }

    #[test]
    fn custom_scheme_url_is_platform_aware() {
        #[cfg(windows)]
        assert_eq!(
            custom_scheme_url("hd-wallpaper", "current?v=1"),
            "http://hd-wallpaper.localhost/current?v=1"
        );
        #[cfg(not(windows))]
        assert_eq!(
            custom_scheme_url("hd-wallpaper", "current?v=1"),
            "hd-wallpaper://current?v=1"
        );
    }

    #[test]
    fn log_dir_is_absolute() {
        assert!(log_dir().is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn owned_tree_spawn_and_graceful_exit() {
        // Spawn `sh -c 'exit 0'` in its own group; graceful terminate must
        // observe a clean exit (this is the "child already exited" path, and
        // proves the tree wraps a real child without error).
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("exit 0");
        OwnedProcessTree::configure_spawn(&mut cmd);
        let child = cmd.spawn().expect("spawn sh");
        let mut tree = OwnedProcessTree::wrap(child);
        // Wait for exit, then graceful_terminate should see it already gone.
        std::thread::sleep(Duration::from_millis(50));
        let out = tree.graceful_terminate(Duration::from_secs(1));
        // Either it exited on its own first (Exited) or was force-killed; it
        // must never be NotOwned for a real pid.
        assert_ne!(out, TerminateOutcome::NotOwned);
    }
}
