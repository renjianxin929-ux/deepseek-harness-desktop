# DeepSeek Harness Desktop V0.2 — Cross-Platform Foundation — Development Spec

> Controlling contract: this document records the V0.2 foundation work. The
> operational prompt (the human-authored execution contract) remains the
> controlling authority; this spec must not silently weaken any rule in it.
> Where implementation reveals a requirement conflict, the conflict is recorded
> here and the run HOLDS instead of rewriting the requirement to make the
> implementation pass.

## 0. Identity

- BASE_HEAD: `483b299305cf91a8079b2eace585d2bfa7b566a8`
- BASE_TAG: `v0.1.0` (points at `8f7abdcf157391e1b49bddb9f92b7ba80e054db3`, an ancestor)
- WORKTREE: `/Users/renjianxin/Harness-Desktop-v0.2-cross-platform-foundation`
- BRANCH: `v0.2/cross-platform-foundation`
- UPSTREAM PRODUCT: DeepSeek Harness Desktop V0.1
- PRODUCT PRINCIPLE: **Keep Harness official. Make it yours.**

## 1. Scope (implement tonight)

- **Piece 0 — Cross-Platform Foundation.** A minimal platform seam so V0.2
  runtime/reliability/usage code does not deepen macOS/Unix lock-in. No Windows
  product is delivered.
- **Piece 1 — Self-Contained Runtime.** Production Harness Desktop uses its own
  bundled Node + pinned Harness runtime. No runtime internet bootstrap at
  startup, no silent fallback to arbitrary system Node/Harness, clear fail-closed
  when the bundled runtime is missing/corrupt, loopback-only binding.
- **Piece 2 — Desktop Reliability.** An explicit reliability state machine,
  owned-process-tree lifecycle, bounded health detection, session-truth
  reconciliation, conservative recovery (never resubmit/cancel/duplicate a task),
  Desktop-restart session continuity, bounded no-loop restart.
- **Piece 3 — Usage + Estimated Cost.** Read-only measured usage from `~/.dsh`
  and a transparently calculated, clearly-labeled estimated cost from a
  versioned pricing snapshot.

## 2. Non-scope (NOT implemented tonight)

Remote Gateway, public listener, `0.0.0.0` exposure, Tailscale, WireGuard,
Cloudflare Tunnel, ngrok, public URL, QR pairing, iOS app, PWA, TestFlight,
App Store, cloud account, cloud sync, notification service, multi-user,
multi-Mac, theme marketplace, new themes, Ocean redesign, Appearance redesign,
KGH, CRM, Agent marketplace, **Windows product release**, **Windows installer
release**. No opportunistic "improvement" of unrelated V0.1 code.

## 3. Architecture decisions

### 3.1 Piece 0 — platform boundary

New module `src-tauri/src/platform.rs` providing cross-platform semantic names:

- `TargetIdentity` — `{ id, os, arch }` where `id ∈ { darwin-arm64, darwin-x64, windows-x64 }`; derived from `cfg!` so it compiles per-target.
- `home_dir()` — `HOME` on Unix, `USERPROFILE` on Windows, with a safe final fallback (never `/etc/passwd`/`/usr/bin/id` on Windows).
- `executable_name(base)` / `node_name()` — appends `.exe` on Windows; identity-preserving.
- `OwnedProcessTree` — the ONLY public process-lifecycle abstraction:
  - `spawn(command) -> (Child, OwnedProcessTree)`
  - `try_wait()`, `id()` (pid), `graceful_terminate(grace)` → `TerminateOutcome::{Exited, ForceKilled, NotOwned}`, `kill_tree()`.
  - Unix: own process group (`process_group(0)`) + `kill(-pgid, SIGTERM)` then bounded `SIGKILL`.
  - Windows: `CREATE_NEW_PROCESS_GROUP` (0x00000200) creation flag; `graceful_terminate` uses a specific-PID tree kill via `taskkill /PID <pid> /T /F` (never `/IM`). Compile-checked only; `WINDOWS_REAL_E2E=NOT_RUN`.
- `capabilities()` — a small platform capability map (e.g. `process_group: true/false`, `signal_kill_group: true/false`).

Refactor seams in `lib.rs` only: replace the local `home_dir()`, the raw
`libc::kill(-pid, …)` group logic, and the `#[cfg(unix)] process_group(0)` with
the `platform` module. Existing V0.1 system-runtime resolution code is KEPT but
relegated to a dev-only path (see 3.2). No broad V0.1 rewrite.

**Windows compatibility audit targets for NEW code:** `/usr/bin` assumptions,
`/bin/zsh`, HOME-only, `/etc/passwd`, hard-coded POSIX separators, Unix-signal
assumptions, `.app`, macOS Application Support, shell-script-only production
deps, executable naming ignoring `.exe`. The legacy system-runtime fallback is
explicitly documented as a macOS/Unix dev path and is not part of the production
path, so its existing Unix assumptions are out of scope for a rewrite.

### 3.2 Piece 1 — bundled runtime

- Pinned Harness: `@deepseek-ai/dsh@0.1.0-rc.6` (unchanged from V0.1; never a
  silent upgrade).
- Pinned Node: **v22.22.3** (exact), chosen from the known-good V0.1
  environment (the machine currently runs Node v22.22.3). Source: official
  `https://nodejs.org/dist/v22.22.3/node-v22.22.3-darwin-arm64.tar.gz`; integrity
  via `SHASUMS256.txt`.
- Resource layout (designed multi-platform from day one, only darwin-arm64
  materialized tonight):

  ```
  runtime/
    manifest.json            # schema + checksums + target table
    NOTICE.md                # third-party notices
    README.md
    darwin-arm64/
      node/bin/node          # pinned Node executable (materialized)
      harness/               # pinned @deepseek-ai/dsh@0.1.0-rc.6 + full dep closure (materialized)
    darwin-x64/              # structurally supported, NOT materialized
    windows-x64/             # structurally supported, NOT materialized
  ```

- Resolution order (production):
  1. Test hooks `HD_FORCE_NODE` / `HD_FORCE_DSH_BIN` (unchanged, tests only).
  2. **Bundled runtime** — located via `HD_RUNTIME_DIR` (test hook) → Tauri
     `resource_dir()/runtime` → next-to-executable → `CARGO_MANIFEST_DIR/../runtime`.
     Manifest validated; Node binary + Harness `lib/bin.js` checksum/version
     checked. This is the ONLY production path.
  3. Legacy system runtime (V0.1) — KEPT but reachable **only** when
     `HD_SYSTEM_RUNTIME=1` (explicit, non-silent dev opt-in). Its status is
     labeled "system (dev-only)". This preserves V0.1 regression coverage
     without reintroducing a silent production fallback.
- Build-time network is used ONLY for exact pinned artifacts (Node tarball +
  `npm install @deepseek-ai/dsh@0.1.0-rc.6` into a staging dir). Startup performs
  NO npm/npx/internet bootstrap.
- `tauri.conf.json` `bundle.resources` includes `../runtime` so the production
  `.app` embeds the runtime.

### 3.3 Piece 2 — reliability state model

Explicit, testable states: `STARTING → READY → HEALTHY`, with `DEGRADED`,
`RECOVERING`, and `FAILED` transitions:

- `STARTING`: resolving runtime, spawning, waiting for port + readiness.
- `READY`: first readiness HTTP 200; WebView navigated.
- `HEALTHY`: periodic bounded health checks pass.
- `DEGRADED`: health checks fail while the owned child is still alive (e.g.
  transient network/sleep-like interruption).
- `RECOVERING`: consecutive health checks failing; polling continues
  conservatively; transition back to `HEALTHY` on recovery.
- `FAILED`: owned child exits unexpectedly OR health checks fail past a bounded
  budget while the child is gone. UI reconciles to the loading page with a
  clear error + Retry. **No automatic restart after FAILED** (avoids duplicate
  task execution); recovery is manual and bounded.

Truth hierarchy: **backend truth > client DOM heuristic**. The V0.1
`session_guard.js` reconciliation (read-only `session.list`, reload on confirmed
completion) is preserved and remains the session-truth mechanism; no DOM
quiet-window logic is reintroduced.

Recovery semantics (hard invariant): restore connection/runtime/UI state only.
Never resubmit a prompt, retry agent work, duplicate a tool call, cancel a task,
or auto-start a second task.

Process ownership: only the `OwnedProcessTree` Desktop spawned is ever signaled.
Unrelated Harness processes are never matched by name/port/HOME and never
touched. No `pkill`/`killall`/`taskkill /IM`.

### 3.4 Piece 3 — usage + estimated cost

- Read-only source of measured usage: `~/.dsh/storages/session_projcache.json`
  (the harness's own persisted `tokenUsage` projection: `uncachedInputTokens`,
  `outputTokens`, `cacheReadTokens`, `cacheWriteTokens`) and `~/.dsh` session
  logs (zstd JSONL) for model identity (`request/context` → `provider`/`model`).
  `~/.dsh` is NEVER modified.
- Model identity: decoded from the durable session log (first `request/context`
  event). If unavailable → shown as "unavailable".
- Current session: reported read-only by a new injected `usage_beacon.js`
  (reads `localStorage["dsh.sessions.current"]`, same official key the V0.1
  session guard uses) via the existing `hd-beacon://` scheme; if not yet
  reported → "unavailable" (never inferred from newest mtime).
- Today: aggregate only sessions whose `identity.createdAt` falls in today's
  local calendar day.
- Cost: versioned pricing snapshot (`pricing-snapshot` v1) with model/provider,
  effective/source date, input-cache-hit/miss + output per-1M-token USD prices,
  currency, source description, and `estimation: true`. Unknown model → measured
  usage still shown, Estimated Cost = unavailable (no price substitution).
- Pricing source: DeepSeek official API pricing page
  (https://api-docs.deepseek.com/quick_start/pricing/, fetched 2026-08-15) —
  `deepseek-v4-pro` and `deepseek-v4-flash`; recorded as a **snapshot**, NOT
  official live billing.
- Cost semantics: `uncachedInputTokens` → cache-miss price;
  `cacheReadTokens + cacheWriteTokens` → cache-hit price; `outputTokens` →
  output price. Reasoning tokens are already a subdivision of output and are not
  double-counted.
- Minimal UI: a small read-only "Usage" window (menu item `Usage…`), no
  settings redesign.

## 4. Windows compatibility contract

- NEW V0.2 code compiles under `cfg(windows)` with no Unix-only imports.
- `OwnedProcessTree` has a Windows implementation (compile-checked; not run).
- `executable_name`/`node_name` map `.exe` correctly.
- `home_dir` uses `USERPROFILE`.
- `resource_dir` resolution uses Tauri's `resource_dir()` (cross-platform).
- Target identity supports `windows-x64` structurally in the runtime manifest.
- Reported statuses: `WINDOWS_ARCHITECTURE_AUDIT=PASS/FAIL`,
  `WINDOWS_CROSS_TARGET_CHECK=PASS/SKIP`, `WINDOWS_BUILD=NOT_RUN`,
  `WINDOWS_REAL_E2E=NOT_RUN`. A cross-target `cargo check` pass is NEVER
  reported as `WINDOWS_BUILD=PASS`.

## 5. Bundled runtime contract

1. Runtime Node is bundled/managed by Desktop.
2. Harness version pinned to `0.1.0-rc.6`.
3. Production startup performs no runtime npm/npx internet bootstrap.
4. Production never silently falls back to arbitrary system Node/Harness.
5. Missing/corrupt bundled runtime → clear fail-closed error.
6. Harness remains bound to `127.0.0.1` dynamic port.
7. `~/.dsh/session` semantics untouched (`DSH_HOME` stays `~/.dsh`).

## 6. Process-ownership safety

- Every spawned test runtime is recorded with run_id/pid/ppid/pgid/cwd/start
  time/purpose.
- A process is never classified as owned solely by being `node`/`dsh`/similar
  cwd/similar HOME/familiar port.
- If ownership cannot be proven: do NOT signal.
- The live harness serving this session (PIDs 47318/47331 and its process
  group) is NEVER signaled; real `~/.dsh` is never modified.

## 7. Test gates

Per Piece: Self Review → Simulated E2E → Adversarial E2E → Repair → Full rerun
(+ V0.1 regression relevant to the Piece) → Freeze (`PIECE_X_STATUS=PASS`).
Max 3 repair cycles per Piece; a still-blocking defect ⇒ `HOLD`.

Repo checks (actual commands): `npm run typecheck`, `npm run build:frontend`,
`(cd src-tauri && cargo test)`, `(cd src-tauri && cargo clippy -- -D warnings)`,
production `npm run tauri build`. V0.1 user-facing areas re-verified at
simulated level: Desktop startup, Harness readiness, official UI, appearance,
themes, wallpaper persistence path, motion/reduced-motion, compatibility
fallback, session guard, clean owned-process shutdown.

## 8. Evidence levels

- L1 internal/unit (pure functions, parsers, manifests, cost math, path logic).
- L2 simulated E2E (production-like Desktop, isolated HOME, restricted PATH,
  bundled runtime startup, real readiness, automatable WebView boundary).
- L3 real human E2E — out of scope; `HUMAN_E2E=PENDING`.

## 9. Stop conditions

Baseline mismatch; Harness Core / upstream patch required; arbitrary `~/.dsh`
modification; broad process killing; license blocker; admin/root/system
mutation; public networking; Remote work; architecture outside scope; 3 repair
cycles fail; unrepaired V0.1 regression; unprovable security boundary; Windows
needs broad unrelated rewrite. On HOLD: write the report, leave tree intact,
STOP. (`FINAL_STATUS=HOLD_<REASON>`.)

## 10. Known limitations (accepted, not defects)

- Windows path is architecture-audited and cross-target compile-checked only;
  no real Windows build/E2E tonight.
- `~/.dsh/storages/session_projcache.json` is a persisted projection cache and
  may lag a live in-flight session by a few events; it is the harness's own
  accounting and is presented as "measured usage (from Harness projection)".
- Pricing is a dated snapshot, an estimate, not official billing; the DeepSeek
  page also announces peak/off-peak pricing effective 2026-08-16 16:00 UTC
  which is recorded but not applied to the snapshot's flat rates.
- The legacy system-runtime resolution remains macOS/Unix-oriented and is
  dev-only (`HD_SYSTEM_RUNTIME=1`).

## 11. Evidence rules

- A screenshot/rendered button is never proof of function.
- A direct inner-executable launch is never equated with a real packaged launch
  where packaging matters (the production `tauri build` bundle is exercised).
- L2 is never reported as human E2E; test hooks never satisfy a user-E2E item.
- `WINDOWS_CROSS_TARGET_CHECK=PASS` is never `WINDOWS_BUILD=PASS`.

## 12. Git rules

Inspect only. No commit/push/tag/merge/rebase/PR/release. All product changes
left uncommitted in the worktree for human review.
