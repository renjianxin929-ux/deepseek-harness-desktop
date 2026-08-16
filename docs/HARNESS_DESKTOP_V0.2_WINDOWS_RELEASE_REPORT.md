# DeepSeek Harness Desktop V0.2 — Windows x64 Beta Release Report

> Controlling contract: the operational prompt (single-agent, strict-serial,
> evidence-based, Windows-only) remains the controlling authority. No Windows
> build/installer/E2E PASS is claimed without Windows CI evidence.

## Summary

- BASE_HEAD=e1ea9c30b6e0b13c67288d660da9c69db6f21a81
- BRANCH=v0.2/windows-release
- FINAL_STATUS=READY_FOR_WINDOWS_CI_PUSH

The Windows x64 Beta release path is prepared locally: Windows runtime
materialization, bundled `node.exe`, pinned `@deepseek-ai/dsh@0.1.0-rc.6`,
integrity metadata, Windows process ownership, Windows-native bundle config, a
GitHub Actions Windows build workflow, NSIS installer production, a Windows
runtime smoke check, and release documentation. The only outstanding requirement
is pushing `v0.2/windows-release` to GitHub to trigger the real Windows build —
that evidence has not yet been produced, so no Windows PASS is fabricated here.

## Target & versions

- WINDOWS_TARGET=windows-x64 (x86_64-pc-windows-msvc)
- NODE_VERSION=22.22.3
- HARNESS_VERSION=0.1.0-rc.6

## Windows runtime materialization (W1)

- WINDOWS_MATERIALIZER=PASS
- WINDOWS_RUNTIME_LAYOUT=PASS
- WINDOWS_INTEGRITY=PASS
- WINDOWS_LICENSE_INVENTORY=PASS

`scripts/materialize-runtime.mjs` already declared a `windows-x64` target
(`node-v22.22.3-win-x64.zip` → `node.exe`). The one Windows defect was that
`run()` used `execFileSync("npm", …)`, which fails on Windows because `npm`
resolves to a `.cmd` shim. It now routes through `cmd.exe /d /s /c` on win32
(also valid for `tar.exe`), keeping the exact same pinned commands and layout.

Verified on this machine (macOS structural inspection — NOT Windows E2E):

- Official Node Windows x64 archive downloaded and SHA-256 verified against the
  official `SHASUMS256.txt` (archive digest
  `6c8d54f635feff4df76c2ca80f45332eb2ff57d25226edce36592e51a177ee33`).
- `runtime/windows-x64/node/node.exe` present (86,969,160 bytes) + `LICENSE`.
- `runtime/windows-x64/harness/` reproduced from the committed
  `scripts/runtime-package-lock.json` via `npm ci`; resolved
  `@deepseek-ai/dsh@0.1.0-rc.6` verified exactly.
- `runtime/manifest.json` written with `nodeSha256` + `harnessEntrySha256` for
  `windows-x64`; `scripts/smoke-windows-runtime.mjs` re-verified both checksums
  and the layout (PASS).
- `runtime/NOTICE.md` + `runtime/THIRD_PARTY_LICENSES.json` generated (525
  packages, 2 LGPL-bearing explicitly listed).

The generated `runtime/` tree is gitignored (not committed).

## Windows source / platform audit (W2)

- WINDOWS_SOURCE_AUDIT=PASS
- WINDOWS_CROSS_TARGET_CHECK=SKIP (see below)

Changes, all Windows-line and none touching the frozen Foundation worktree:

- `src-tauri/Cargo.toml`: `libc` + `signal-hook` moved to
  `[target.'cfg(unix)'.dependencies]` (referenced only under `#[cfg(unix)]`),
  removing them from the Windows build surface. Package description now
  cross-platform.
- `src-tauri/src/platform.rs`: added `custom_scheme_url()` (macOS
  `<scheme>://…` vs Windows `http://<scheme>.localhost/…`) and `log_dir()`
  (macOS `~/Library/Logs/HarnessDesktop` vs Windows
  `%LOCALAPPDATA%\HarnessDesktop\logs`), with tests.
- `src-tauri/src/lib.rs`: startup log path now via `platform::log_dir()`;
  appearance navigation guard accepts both scheme forms; test-only
  `std::os::unix::fs::symlink` import + test gated `#[cfg(unix)]`.
- `src-tauri/src/appearance.rs`: wallpaper custom-scheme URL now via
  `platform::custom_scheme_url()`; three POSIX-symlink tests gated `#[cfg(unix)]`.
- `src-tauri/src/runtime.rs`: test fixtures made platform-aware
  (`<id>/node/node.exe` on Windows vs `<id>/node/bin/node` on macOS); the one
  test that must execute a runnable fake node is gated `#[cfg(unix)]` (Windows
  exercises the real `node.exe` via the CI smoke script).

Audit result: no unconditional `std::os::unix` usage remains in product code;
no `pkill`/`killall`/`taskkill /IM`/`Stop-Process`; Windows process lifecycle
remains exact-owned-PID tree only (`taskkill /PID <pid> /T /F` via
`OwnedProcessTree`, plus `CREATE_NEW_PROCESS_GROUP`).

WINDOWS_CROSS_TARGET_CHECK: a full `cargo check --target
x86_64-pc-windows-msvc` on macOS is blocked at the native C dependency
`zstd-sys` (its `cc` build requires a Windows C toolchain: `'string.h' file not
found`). That is a host toolchain limitation, not a source defect. As
compensating static evidence, the Windows-only code paths were extracted and
type-checked in isolation with `rustc --target x86_64-pc-windows-msvc
--emit=metadata` → clean (exit 0), covering `windows_home_dir`, `configure_spawn`
(`creation_flags`), `windows_graceful_terminate` (`taskkill` args), the Windows
`log_dir`, and the Windows `custom_scheme_url` form. The authoritative Windows
compile/package happens on `windows-latest` in CI.

## GitHub Actions workflow (W3)

- GITHUB_ACTIONS_WORKFLOW=READY

Created `.github/workflows/windows-release.yml` (`windows-x64` job,
`runs-on: windows-latest`, 14 steps):

checkout → setup Node 22.22.3 → setup Rust stable (clippy) → `npm ci` →
typecheck → materialize Windows runtime → Windows runtime smoke test → engine
tests → session-guard tests → `build:frontend` → `cargo clippy -- -D warnings`
→ `cargo test` → `tauri build --bundles nsis` → upload NSIS `.exe` +
`manifest.json` + `NOTICE.md` + `THIRD_PARTY_LICENSES.json`
(`if-no-files-found: error`). Triggers on push to `v0.2/windows-release` and on
`workflow_dispatch`. YAML validated with js-yaml.

- WINDOWS_BUILD=NOT_RUN
- WINDOWS_INSTALLER=NOT_RUN
- WINDOWS_REAL_E2E=NOT_RUN

(No Windows CI run exists yet because the branch has not been pushed.)

## Local verification (macOS)

- TYPECHECK=PASS
- FRONTEND_BUILD=PASS
- ENGINE_TESTS=PASS
- GUARD_TESTS=PASS
- RUST_TESTS=PASS (119 passed; 0 failed)
- CLIPPY=PASS (`cargo clippy -- -D warnings`)

`cargo test`/`cargo clippy` require the `runtime/` resource directory to exist
(`tauri.conf.json` `bundle.resources: ["../runtime"]`); it is materialized/gitignored
and the CI materializes it before the Rust steps. Note: `cargo clippy
--all-targets -- -D warnings` additionally surfaces two pre-existing, non-Windows
test-code lints (`items_after_test_module` in `appearance.rs`, a test-only
`clone` in `lib.rs`); the task's exact command (`cargo clippy -- -D warnings`)
passes and is what the workflow uses.

## Product / security invariants

- REMOTE_IMPLEMENTED=false
- PUBLIC_HARNESS_LISTENER=false (Harness stays bound to `127.0.0.1` dynamic port;
  no `0.0.0.0`).
- SUBAGENT_USAGE=0
- WORKFLOW_USAGE=0
- BROAD_PROCESS_KILL_USED=false

## Git

- COMMIT_CREATED=false
- PUSH_PERFORMED=false
- TAG_CREATED=false

## Changed files

- package.json
- scripts/materialize-runtime.mjs
- scripts/smoke-windows-runtime.mjs (new)
- src-tauri/Cargo.toml
- src-tauri/src/appearance.rs
- src-tauri/src/lib.rs
- src-tauri/src/platform.rs
- src-tauri/src/runtime.rs
- src-tauri/tauri.windows.conf.json (new)
- .github/workflows/windows-release.yml (new)
- docs/HARNESS_DESKTOP_V0.2_WINDOWS_RELEASE_REPORT.md (new)

## Known limitations

- The local materialization of `windows-x64` on macOS runs `npm ci` on the macOS
  host, so the `harness/node_modules` it produced is macOS-flavored (e.g.
  `sharp` darwin binary). The authoritative Windows `harness/node_modules`
  (Windows optional deps such as `@img/sharp-win32-x64`) is produced by the same
  script on `windows-latest` in CI. The macOS run is a structural/layout +
  integrity check only — never reported as a Windows build.
- The injected appearance/usage *beacon* URLs (`hd-beacon://…`,
  `hd-usage-session://…`, and the appearance-button `hd-appearance://open`) are
  still hard-coded macOS-style custom-scheme URLs in the injected JS. They are
  diagnostic/cosmetic, fail silent, and do not affect the Harness launch path;
  the wallpaper URL (the user-visible asset) and the navigation guard are
  platform-aware. Fixing the remaining JS beacons is deferred, non-blocking.
- `cargo check --target x86_64-pc-windows-msvc` cannot run to completion on
  macOS (native C dependency `zstd-sys`); see W2 note.
- The legacy dev-only system-runtime resolver (`HD_SYSTEM_RUNTIME=1`) remains
  macOS/Unix-oriented by design; it is not the production path and is out of
  scope.

## Next human action

Push `v0.2/windows-release` to GitHub (or run the workflow via
`workflow_dispatch`) to trigger the authoritative Windows x64 build, NSIS
installer production, and artifact upload. Inspect the uploaded installer +
runtime metadata, then perform a real Windows 10/11 install + launch E2E.
