# DeepSeek Harness Desktop V0.2 — Overnight Execution Report

BASE_HEAD=483b299305cf91a8079b2eace585d2bfa7b566a8
WORKTREE=/Users/renjianxin/Harness-Desktop-v0.2-cross-platform-foundation
BRANCH=v0.2/cross-platform-foundation
FINAL_STATUS=PASS_IMPLEMENTATION_PENDING_HUMAN_E2E

---

## 1. Per-Piece status

### PIECE 0 — Cross-Platform Foundation

PIECE_0_PLATFORM_STATUS=PASS
SELF_REVIEW_ROUNDS=2
ISSUES_FOUND=2
ISSUES_FIXED=2
UNIT_INTERNAL=PASS
SIMULATED_E2E=PASS
ADVERSARIAL_E2E=PASS
V0_1_REGRESSION=PASS

What was built: `src-tauri/src/platform.rs` — `TargetIdentity` (darwin-arm64 /
darwin-x64 / windows-x64), `home_dir()` (HOME vs USERPROFILE), `executable_name()`
/ `node_name()` (`.exe` mapping), `path_separator()`, `app_origin_url()`,
`Capabilities`/`capabilities()`/`describe()`, and `OwnedProcessTree` (the public
owned-process-tree lifecycle: `configure_spawn`, `wrap`, `try_wait`, `wait`,
`graceful_terminate`, `kill_tree`). `lib.rs` was refactored to use it (removed the
local `home_dir`, the raw `libc::kill(-pid)` group logic, and the `#[cfg(unix)]
process_group(0)` block).

Issues found/fixed (all during the implement→test loop, none exceeded a repair
cycle): (1) forward-declared APIs triggered `-D warnings` dead-code — fixed by a
startup `describe()` diagnostic; (2) clippy `needless_return` in the cfg-split
`home_dir` — fixed.

### PIECE 1 — Self-Contained Runtime

PIECE_1_RUNTIME_STATUS=PASS
SELF_REVIEW_ROUNDS=3
ISSUES_FOUND=3
ISSUES_FIXED=3
UNIT_INTERNAL=PASS
SIMULATED_E2E=PASS
ADVERSARIAL_E2E=PASS
V0_1_REGRESSION=PASS

What was built: `src-tauri/src/runtime.rs` (manifest validation, target
resolution, SHA-256 verification helper, node version probe) plus a materialized
`runtime/` tree: `darwin-arm64/node/bin/node` (pinned Node v22.22.3) and
`darwin-arm64/harness/` (pinned `@deepseek-ai/dsh@0.1.0-rc.6` + full dependency
closure, 531 packages), `manifest.json` (checksums + per-target table),
`NOTICE.md`, `README.md`, and the `darwin-arm64/node/LICENSE`. `tauri.conf.json`
bundles `../runtime`. `lib.rs::resolve_runtime` now prefers the bundled runtime
(production) and gates the V0.1 system fallback behind `HD_SYSTEM_RUNTIME=1`.

Issues found/fixed: (1) serde camelCase/snake_case manifest mismatch (caught by
simulated E2E — "missing field schema_version"); (2) Tauri bundler places
`..`-prefixed resources under `_up_/` (caught by bundle inspection) — added the
`_up_` candidate; (3) unused `node_version`/`harness_version`/`source` fields —
folded into the `source` string.

### PIECE 2 — Desktop Reliability

PIECE_2_RELIABILITY_STATUS=PASS
SELF_REVIEW_ROUNDS=2
ISSUES_FOUND=2
ISSUES_FIXED=2
UNIT_INTERNAL=PASS
SIMULATED_E2E=PASS
ADVERSARIAL_E2E=PASS
SOAK_DURATION_MINUTES=60
SOAK_RESULT=PASS
V0_1_REGRESSION=PASS

What was built: `src-tauri/src/reliability.rs` (pure, testable state machine:
STARTING → READY → HEALTHY with DEGRADED / RECOVERING / FAILED) and a bounded
health monitor in `lib.rs` (5 s loopback health polls; child exit is
authoritative; `Failed` is terminal — recovery restores connection/UI only, never
resubmits/cancels/duplicates). On `Failed` the WebView reconciles back to the
loading page with a clear error + Retry. Generation counter + shutdown guard
prevent stale monitors and spurious failures; restart is manual and bounded.

Issues found/fixed: (1) `&str`→`String` mismatch in `error_payload` (compile);
(2) a spurious `FAILED` could be reported during clean shutdown — added the
`SHUTTING_DOWN` guard in the monitor loop.

### PIECE 3 — Usage + Estimated Cost

PIECE_3_USAGE_COST_STATUS=PASS
SELF_REVIEW_ROUNDS=2
ISSUES_FOUND=2
ISSUES_FIXED=2
UNIT_INTERNAL=PASS
SIMULATED_E2E=PASS
ADVERSARIAL_E2E=PASS
V0_1_REGRESSION=PASS

What was built: `src-tauri/src/usage.rs` (read-only measured usage from
`~/.dsh/storages/session_projcache.json` + zstd session logs; versioned pricing
snapshot v1; cost math), `src-tauri/src/usage_beacon.js` (read-only current-session
beacon), `usage.html` + `src/usage.ts` + `src/usage.css` (minimal read-only Usage
window), wired via a `hd-usage-session` URI scheme, a `Usage…` menu item, the
`get_usage` command, and an `HD_OPEN_USAGE` test hook.

Issues found/fixed: (1) serde generic `Deserialize` bound error on the projcache
row wrapper (fixed by removing the generic); (2) "today" cost could be computed
over only a *known-model subset* of tokens — fixed to require every contributing
session to have a known, shared, priced model (no partial pricing, no price
substitution), locked in by `compute_usage_partial_model_coverage_does_not_price_unknown`.

---

## 2. Platform status

MACOS_IMPLEMENTATION=PASS
MACOS_AUTOMATED_E2E=PASS

WINDOWS_ARCHITECTURE_AUDIT=PASS
WINDOWS_CROSS_TARGET_CHECK=FAIL
WINDOWS_BUILD=NOT_RUN
WINDOWS_REAL_E2E=NOT_RUN

HUMAN_E2E=PENDING

Windows notes (honest): the new `platform` module's `#[cfg(windows)]` branches
were compile-checked in isolation against `x86_64-pc-windows-msvc` (PASS; the
isolated check compiled `platform.rs` verbatim). The **full-crate**
`cargo check --target x86_64-pc-windows-msvc` failed for an **environmental**
reason, not a code defect: Tauri's `tauri-winres` build script requires
`llvm-rc` (the Windows resource compiler), which is not present on this macOS
host. A real Windows build requires Windows and is therefore `NOT_RUN`. The
legacy system-runtime fallback remains macOS/Unix-oriented and is dev-only.

---

## 3. Agent usage

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0

---

## 4. Process safety

BROAD_PROCESS_KILL_USED=false

Only processes explicitly spawned and recorded by this run were signalled
(specific PIDs, and specific owned process-groups via `OwnedProcessTree`). The
live Harness Desktop instance serving this session (PID 47318/47331 and its
tree) was never signalled, and the real `~/.dsh` was never modified. One
orphaned soak child (PID 73418, created by an aborted first soak attempt) was
terminated by its specific recorded PID after ownership was confirmed from its
command line.

---

## 5. Remote

REMOTE_IMPLEMENTED=false
PUBLIC_LISTENER_CREATED=false

The Harness remains bound to `127.0.0.1` (dynamic port) only.

---

## 6. Git

COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

All product changes are left uncommitted in the worktree for human review.

---

## 7. Changed files

Modified:
- `README.md` — bundling statement (no longer "does not bundle Harness"), requirements, bundled-runtime section, roadmap, features (reliability + usage), Chinese mirror.
- `src-tauri/Cargo.toml` — added `zstd = "0.13"`.
- `src-tauri/Cargo.lock` — zstd + transitive deps.
- `src-tauri/src/lib.rs` — platform seam, bundled-runtime resolution, reliability monitor + reconcile, usage wiring (URI scheme, menu, command, window, beacon injection).
- `src-tauri/tauri.conf.json` — `bundle.resources: ["../runtime"]`.
- `vite.config.ts` — added `usage` input.

New:
- `docs/HARNESS_DESKTOP_V0.2_CROSS_PLATFORM_FOUNDATION_DEV_SPEC.md`
- `src-tauri/src/platform.rs`
- `src-tauri/src/runtime.rs`
- `src-tauri/src/reliability.rs`
- `src-tauri/src/usage.rs`
- `src-tauri/src/usage_beacon.js`
- `src/usage.ts`, `src/usage.css`, `usage.html`
- `runtime/` (manifest.json, NOTICE.md, README.md, darwin-arm64/node + harness)

---

## 8. Architecture changes

- New `platform` seam (cross-platform semantic names; `OwnedProcessTree`).
- New `runtime` module (bundled runtime resolution; production fail-closed).
- New `reliability` module (explicit health state machine + bounded monitor).
- New `usage` module (read-only usage + dated pricing snapshot).
- `lib.rs` refactored around these four seams; the V0.1 system-runtime path is
  preserved but demoted to a dev-only (`HD_SYSTEM_RUNTIME=1`) fallback.

---

## 9. Exact tests executed

- `npm run typecheck` — PASS.
- `npm run build:frontend` — PASS (dist includes index/appearance/usage).
- `npm run test:engine` — PASS.
- `npm run test:guard` — PASS.
- `cargo test` — **104 passed, 0 failed** (incl. platform, runtime, reliability,
  usage unit + fixture tests, plus all existing V0.1 appearance/css_scope/
  session_guard/resolver tests).
- `cargo clippy -- -D warnings` — PASS.
- `npm run tauri build` (production `.app`) — PASS (bundle at
  `src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app`).
- Isolated `platform.rs` compile vs `x86_64-pc-windows-msvc` — PASS.
- Full `cargo check --target x86_64-pc-windows-msvc` — FAIL (environmental,
  `llvm-rc` missing in `tauri-winres`; not a code defect).

Simulated E2E (L2), all with isolated HOME + restricted PATH (no system
node/npm/npx/dsh) and loopback dynamic port:
- Bundled-runtime readiness + official UI HTTP 200 (debug binary and production
  `.app`; verified `window.__DSH_BOOT__` in the served HTML).
- Adversarial runtime: corrupt manifest, missing node, missing harness entry,
  wrong harness version, runtime-not-found → all fail closed with clear errors.
- `HD_SYSTEM_RUNTIME=1` dev fallback → still reaches ready (via npx-cache symlink,
  no network, no real `~/.dsh`).
- Reliability: SIGKILL the owned harness child → `HEALTHY → FAILED` + error +
  UI reconcile; Desktop restart continuity (same isolated HOME → `.dsh` persists,
  second ready).
- Usage: `get_usage`/window open with empty isolated HOME ("unavailable") and with
  a **copy** of real `~/.dsh` data (no crash; real `~/.dsh` untouched).
- Appearance + Usage windows open together (V0.1 appearance regression).

Real-data read-only probe (one-off, removed): `compute_usage` against the real
`~/.dsh` returned today's usage — input 3,539,843 / output 1,506,927 /
cache_read 291,060,864 / cache_write 0 (total 296,107,634), model
`deepseek-v4-pro`, 1,676 API calls, estimated cost ≈ $3.9059.

---

## 10. Failing tests

None at final freeze. (The full-crate Windows cross-target `cargo check` fails
only because `llvm-rc` is absent on this host; this is documented above and is
not a source defect.)

---

## 11. Repair history

Piece 0: dead-code/needless_return clippy fixes. Piece 1: serde camelCase fix,
`_up_` resource-path fix, unused-field cleanup. Piece 2: `&str`→`String` fix,
shutdown-guard fix (spurious FAILED). Piece 3: serde generic-bound fix,
partial-model-coverage cost fix, usage-window error handling. No piece required
more than its allowed repair budget; all froze PASS.

---

## 12. Known limitations

- Windows path is architecture-audited and compile-checked only; no real Windows
  build/E2E tonight.
- `~/.dsh/storages/session_projcache.json` is the harness's persisted projection
  cache and may lag a live in-flight session slightly; it is presented as
  "measured usage (from Harness projection)".
- Pricing is a dated snapshot (an estimate), not official billing; the DeepSeek
  page also announces peak/off-peak pricing effective 2026-08-16 16:00 UTC, which
  is recorded in the snapshot note but not applied to the flat rates used.
- The startup log path (`~/Library/Logs/HarnessDesktop/startup.log`) is a
  pre-existing macOS path and was not part of the platform-boundary rewrite.
- The `runtime/` binaries are materialized in the worktree but uncommitted; the
  human reviewer decides whether to commit them, add a pinned fetch script, or
  gitignore them (see `runtime/README.md` / `runtime/NOTICE.md`).
- Session "reuse" continuity was verified structurally (DSH_HOME unchanged,
  isolated `.dsh` persists across restart); a full end-user session-reuse test
  needs human interaction.

---

## 13. Unresolved risks

- Human E2E has not been performed (`HUMAN_E2E=PENDING`).
- Windows compatibility cannot be proven without a Windows machine.
- The `hd-usage-session` beacon extracts the current-session id by simple string
  split (session ids are UUID-shaped); a future non-UUID session id containing
  `&`/`%` would need proper URL query decoding.

---

## 14. Runtime versions / checksums

- Node: v22.22.3 (pinned), source
  `https://nodejs.org/dist/v22.22.3/node-v22.22.3-darwin-arm64.tar.gz`;
  tarball SHA-256 `0da7ff74ef8611328c8212f17943368713a2ad953fb7d89a8c8a0eae87c23207`;
  binary SHA-256 `5d9d3872911e2340a43b707962e68143de8a4e8d54628845c0c4f2de1fb7cd5c`.
- Harness: `@deepseek-ai/dsh@0.1.0-rc.6` (pinned); entry
  `node_modules/@deepseek-ai/dsh/lib/bin.js` SHA-256
  `c0226687bb20f45c603ec6fe50f3de16d1c3510c3a803304ec575ef9bc366c62`.
- These are recorded in `runtime/manifest.json` and verified at startup via
  file/version checks (full checksum verification via `HD_VERIFY_RUNTIME=1`).

---

## 15. Licensing changes

V0.1 described a non-bundled upstream relationship. V0.2 distributes a pinned,
unmodified `@deepseek-ai/dsh@0.1.0-rc.6` runtime (MIT) and a pinned Node binary
(MIT-style), both redistribution-permitted with attribution. `runtime/NOTICE.md`
records the bundling statement; each package's own `LICENSE` is preserved under
`runtime/darwin-arm64/harness/node_modules/**`, and Node's license is preserved at
`runtime/darwin-arm64/node/LICENSE`. `README.md` was updated so it no longer
claims "does not bundle Harness Core". No license blocker was encountered.

---

## 16. Windows compatibility limitations

No Windows product is delivered. The cross-platform seams (`platform`, `runtime`
manifest targets, `OwnedProcessTree`) are present and Windows-aware; the full
cross-target check is blocked only by the missing `llvm-rc` tool. A real Windows
build (`WINDOWS_BUILD=NOT_RUN`) and real E2E (`WINDOWS_REAL_E2E=NOT_RUN`) remain.

---

## 17. Usage data source

Read-only from the user's `~/.dsh`: token buckets from
`~/.dsh/storages/session_projcache.json` (`tokenUsage` projection:
uncachedInputTokens / outputTokens / cacheReadTokens / cacheWriteTokens), and
model identity + API-call count from the durable `session.jsonl.zstd` logs
(`request/context` for provider/model; `assistant/message` for call count).
Current-session identity comes only from the officially-persisted
`localStorage["dsh.sessions.current"]` via the read-only beacon; otherwise it is
"unavailable" (never inferred from timestamps).

---

## 18. Cost pricing source / snapshot

Versioned snapshot `pricing-snapshot` v1, sourced from the DeepSeek official API
pricing page (https://api-docs.deepseek.com/quick_start/pricing/, fetched
2026-08-15), USD per 1M tokens: `deepseek-v4-pro` cache-hit $0.003625 / cache-miss
$0.435 / output $0.87; `deepseek-v4-flash` cache-hit $0.0028 / cache-miss $0.14 /
output $0.28. Every entry is flagged `estimation: true`. Calculation:
uncached input → cache-miss rate; cache read+write → cache-hit rate; output →
output rate (reasoning is already a subdivision of output and is not
double-counted). Unknown model/price ⇒ usage shown, cost "unavailable".

---

## 19. Regression risks

Low. All V0.1 unit/engine/guard tests pass unchanged; the appearance system,
session guard, and system-runtime code were not modified (the system-runtime path
is only gated behind `HD_SYSTEM_RUNTIME=1`). The main behavioral change is that
production startup now requires the bundled runtime instead of system
Node/Harness — this is the intended V0.2 change and fails closed rather than
silently falling back.

---

## 20. Recommended human E2E for tomorrow

1. Launch the production `.app` and confirm the official Harness UI opens.
2. Open **DeepSeek Harness Desktop ▸ Usage…** and verify Current Session / Today
   figures and Estimated Cost render for your real usage; confirm the label reads
   "estimated", not "bill".
3. Switch a session (or open a subagent) and confirm the Current Session card
   follows the selection.
4. Quit the app and confirm the Harness child is cleanly shut down (no orphan
   node process, no lingering port).
5. Optionally, review the bundled runtime and decide whether to commit the
   `runtime/` binaries, add a pinned fetch script, or gitignore them.
6. (Windows, later) build on Windows and run real E2E — tonight's audit is
   architecture-level only.
