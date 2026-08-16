# Harness Desktop V0.2 — Windows Real-E2E P1 Fix

FINAL_STATUS=READY_FOR_WINDOWS_HOTFIX_CI

## Summary

The Windows NSIS package builds and installs, and the loading UI opens, but the
packaged app fails at the version probe with:

- `Harness failed to start`
- `Version: unknown`
- `Version probe failed: version probe produced no output.`
- Node stderr: `Error: EISDIR: illegal operation on a directory, lstat 'E:'`
  through `realpathSync → Module._findPath → resolveMainPath → executeUserEntryPoint`

This is fixed by normalizing the Windows verbatim / extended-length path
(`\\?\E:\...`) **only at the process-execution boundary**, while keeping the
verbatim path for all filesystem/integrity resolution.

## Root cause (proven, not assumed)

- `Tauri resource_dir()` on Windows returns a verbatim path, e.g.
  `\\?\E:\DeepSeek Harness Desktop\_up_\`.
- The bundled-runtime resolver (`runtime::resolve`) keeps this raw path for
  `is_file`, `read`, and SHA-256 checksum. All of those succeed with the
  verbatim path, which is why materialization/checksum/locate all PASS.
- The first failure is the **Harness version probe** in `lib.rs::probe_version`,
  which runs `node <bin_js> --version`. The `<bin_js>` argument is the raw
  verbatim path `\\?\E:\...\lib\bin.js`.
- Node's main-module resolver (`executeUserEntryPoint → resolveMainPath →
  Module._findPath → realpathSync`) mis-parses the `\\?\` prefix and ends up
  `lstat`-ing the drive root `E:` (a directory), hence `EISDIR`.
- Confirming boundary: the earlier `runtime::probe_node_version` runs
  `node --version` (no script argument) and **succeeds** with the same verbatim
  executable path. That proves the verbatim *executable* path spawns fine while
  the verbatim *entry-script* path (argv[1]) is what Node rejects.

First incorrect process-boundary value: the **Harness entry script passed as
`argv[1]`** (`\\?\E:\...\bin.js`), closely followed by the same verbatim entry
path (and runtime-derived `PATH` dir) in the real Harness child spawn.

## Fix

One explicit platform helper — `platform::process_path()` (backed by the pure
string transform `platform::strip_verbatim_prefix()`):

- `\\?\E:\foo\bar` → `E:\foo\bar`
- `\\?\UNC\server\share\foo` → `\\server\share\foo` (UNC preserved, never
  reduced to a drive path)
- everything else (ordinary path, plain UNC, non-Windows path) unchanged

`process_path` is applied only where a path crosses into a spawned process:

1. `runtime::probe_node_version` — the node executable.
2. `lib::probe_version` — node executable + entry script / npx.
3. `lib::start_flow` — node executable + entry script + runtime-derived `PATH`
   directory.

Filesystem/integrity resolution (`is_file`, `read`, checksum, `canonicalize`)
still uses the raw verbatim path — unchanged. No shell execution introduced
(all launches remain direct `Command`), and no broad process kill added.

## Required regression tests

Rust unit tests in `platform.rs` cover:

- ordinary `C:\Program Files\DeepSeek Harness Desktop\node.exe` (unchanged)
- verbatim drive `\\?\C:\Program Files\DeepSeek Harness Desktop\node.exe`
- verbatim UNC `\\?\UNC\server\share\DeepSeek Harness Desktop\node.exe`
- non-Windows paths (`/usr/local/bin/node`) unchanged
- `process_path` identity on non-Windows + boundary strip on Windows

Windows-native smoke test (`scripts/smoke-windows-runtime.mjs`) strengthened so
GitHub Actions executes (not just stat-checks) the bundled runtime:

- `node.exe --version` from a path containing spaces
- `node.exe <entry probe>` (representative of the Harness launch shape) from a
  path containing spaces
- plus the existing real `dsh --version`

## Verification (run on this macOS host)

- TYPECHECK=PASS
- FRONTEND_BUILD=PASS
- ENGINE_TESTS=PASS
- GUARD_TESTS=PASS
- RUST_TESTS=PASS (124 passed)
- CLIPPY=PASS (`cargo clippy -- -D warnings`, no warnings)
- Windows smoke (structural, macOS): PASS; functional branch correctly skipped
  on `darwin` (node.exe is a Windows binary)

The final Windows packaged-app E2E cannot be run from macOS and is **not**
claimed here; it is the human-controlled step after this CI hotfix.

## Report fields

ROOT_CAUSE=Tauri resource_dir() returns a Windows verbatim path (`\\?\E:\...`); the bundled-runtime resolver keeps it for filesystem/integrity (works) but also passes it to Node as the entry-script argument (argv[1]), which Node's main-module resolver mis-parses and `lstat`s the drive root `E:` (EISDIR).

VERBATIM_PATH_INVOLVED=\\?\E:\DeepSeek Harness Desktop\_up_\runtime\windows-x64\...

RAW_NODE_PATH=\\?\E:\DeepSeek Harness Desktop\_up_\runtime\windows-x64\node\node.exe

PROCESS_NODE_PATH=E:\DeepSeek Harness Desktop\_up_\runtime\windows-x64\node\node.exe

PROCESS_CWD=%USERPROFILE% (platform::home_dir(); ordinary Win32 path, not verbatim, left unchanged)

FIX=Single platform helper platform::process_path()/strip_verbatim_prefix() strips the verbatim prefix only at process-execution boundaries (node executable, entry script, runtime-derived PATH dir).

PROCESS_PATH_BOUNDARY=Command::new(), Command::arg() (argv[1] entry), and the PATH dir derived from node.parent() in probe_node_version / probe_version / start_flow.

UNC_HANDLING=\\?\UNC\server\share\... → \\server\share\... (preserved; never reduced to a drive path).

VERSION_PROBE_FIX=Both runtime::probe_node_version (node --version) and lib::probe_version (node bin.js --version) normalize executable + entry before spawning.

HARNESS_SPAWN_FIX=lib::start_flow normalizes node executable + entry script + runtime-derived PATH dir before spawning the real Harness child.

WINDOWS_PATH_TESTS=platform.rs: strip_verbatim_prefix_ordinary_path_unchanged, strip_verbatim_prefix_verbatim_drive, strip_verbatim_prefix_verbatim_unc, strip_verbatim_prefix_non_windows_unchanged, process_path_identity_and_boundary.

WINDOWS_RUNTIME_EXECUTION_SMOKE=scripts/smoke-windows-runtime.mjs now runs bundled node.exe --version and node.exe <entry probe> from a space-containing path (plus real dsh --version) on win32.

TYPECHECK=PASS
FRONTEND_BUILD=PASS
ENGINE_TESTS=PASS
GUARD_TESTS=PASS
RUST_TESTS=PASS
CLIPPY=PASS

MAC_REGRESSION_RISK=NONE — process_path() is the identity on non-Windows; all 124 Rust tests and clippy pass on macOS.

KNOWN_LIMITATIONS=1) The Windows cfg branch of process_path is compile-verified only on the Windows CI runner; on macOS the same transform is verified as pure string logic. 2) The real packaged-app Windows E2E (install → launch → READY) is not run here and is the human-controlled final step. 3) The dev-only legacy system-runtime path (HD_SYSTEM_RUNTIME=1, pick_node/probe_npx_version) still uses fs::canonicalize-derived verbatim paths; intentionally out of scope for this P1. 4) Prefix detection uses to_string_lossy (lossless for valid-UTF-8 Windows paths).

CHANGED_FILES=src-tauri/src/platform.rs, src-tauri/src/runtime.rs, src-tauri/src/lib.rs, scripts/smoke-windows-runtime.mjs

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0
BROAD_PROCESS_KILL_USED=false

COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

## Next step

Human-controlled commit/push → real Windows GitHub Actions → one final Windows
human install E2E. STOP here; no Appearance work, no Remote, no release
packaging expansion.
