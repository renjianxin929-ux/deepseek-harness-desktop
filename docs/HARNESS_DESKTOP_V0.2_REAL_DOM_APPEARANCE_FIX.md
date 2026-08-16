# DeepSeek Harness Desktop V0.2 — Real-DOM Appearance Fix

WORKTREE=/Users/renjianxin/Harness-Desktop-v0.2-deep-glass-polish
BRANCH=v0.2/deep-glass-polish
BASE_HEAD=8ea5d8b7e3b2ca5ced9a53c8f3509b3b508d47c2

FINAL_STATUS=READY_FOR_REAL_DOM_HUMAN_E2E

CONTROL_PATH_REOPENED=false

---

## 1. Evidence method (not fake DOM)

I read the ACTUAL bundled rc.6 Harness source and ran the REAL packaged app
(isolated `HOME`) with a `HD_APPEARANCE_SELF_TEST=1` hook plus a new engine
`probeRealDom()` diagnostic that reports real `getComputedStyle()` values via an
`hd-beacon://dom` beacon into the startup log. All figures below are from that
live run, not fixtures.

Real rc.6 structure (from `dsh-web-frontend/dist/index.html`,
`dsh-client-ui-theme/lib/styles/design-platform.css`,
`dsh-client-ui-layout/lib/client.js`):

- `html,body,#root{height:100%}` — `#root` has NO background (transparent).
- `body{background:var(--dsw-alias-bg-base)}`.
- The "root" slot renders: `#root` → empty wrapper `<div>` → `.pI_x6G_frame`
  (the 3-column grid) → `.pI_x6G_sidebarCol` / `.pI_x6G_centerCol` /
  `.pI_x6G_detailsCol`.
- `.pI_x6G_frame{background:var(--dsw-alias-bg-base)}` and
  `.pI_x6G_sidebarCol{background:var(--dsw-specific-sidebar-fill)}`.
- A Harness `ThemePresenter` writes the tokens as inline `body.style` values
  (the engine's `!important` stylesheet overrides still win).

---

## 2. Wallpaper probe (after fix)

WALLPAPER_ELEMENT_EXISTS=PASS
  `#hd-wallpaper` DIV present inside `#hd-cinematic`.
WALLPAPER_RESOURCE_LOAD=PASS (URL set + served)
  computed `background-image: url("hd-wallpaper://current?v=1")`; served through
  the unchanged `hd-wallpaper` scheme (200, image MIME).
WALLPAPER_STACKING=PASS
  `#hd-backdrop` is `position:fixed; inset:0; z-index:-1`; above it the chain is
  `body`(transparent) → `#root`(transparent) → wrapper(transparent) →
  frame(translucent) → sidebar(translucent). No opaque viewport cover remains.

REAL_VIEWPORT_COVERING_ELEMENTS=
  body: background rgba(0,0,0,0) (engine `transparent !important`)
  #root: background rgba(0,0,0,0); backdrop-filter blur(18px) saturate(1.16) @100
  #root > * (empty wrapper): transparent
  #root > * > * (`.pI_x6G_frame`): background var(--dsw-alias-bg-base) → translucent
  .pI_x6G_sidebarCol: background var(--dsw-specific-sidebar-fill) → translucent
  .pI_x6G_centerCol / .pI_x6G_detailsCol: no background (transparent)

REAL_SELECTOR_AUDIT=
  #root                      → matches (id="root" in index.html) — PASS
  --dsw-alias-bg-base        → defined + consumed by body & frame — PASS
  --dsw-alias-bg-layer-1/2/3 → defined + consumed (cards/panels/dialogs) — PASS
  --dsw-alias-bg-module-platform → defined + consumed — PASS
  --dsw-specific-sidebar-fill→ defined + consumed by sidebar — PASS
  --dsw-alias-label-primary  → defined + consumed (CORE_TOKEN gate) — PASS
  (Verified via `tests/harness_compat.test.mjs` reading the real bundled source.)

---

## 3. Root cause and fix

FIRST_VISUAL_BLOCKER=ID_COLLISION_ON_hd-cinematic
  `IDS.cinematic` ("hd-cinematic") was used for BOTH the backdrop wrapper `<div>`
  AND the cinematic-motion `<style>` element. `ensureStyle("hd-cinematic")`
  called `getElementById`, found the **div**, then set `.textContent = cinematicCss()`,
  which replaced the wallpaper/video children with a CSS text node. Result:
  `#hd-cinematic` existed but `#hd-wallpaper`/`#hd-video` were gone → wallpaper
  never rendered (while the state beacon still reported `wallpaper=1`, which is
  why control-path checks looked "green").

SECONDARY_VISUAL_BLOCKERS=
  The ambient decorative asset targets `#root > *` (the empty wrapper), not the
  frame `#root > * > *`. This affects the NO-wallpaper ambient gradient only
  (not wallpaper/glass depth) and is left unchanged to avoid scope expansion.

FIX=SPLIT_THE_ID
  Added `IDS.cinematicStyle = "hd-cinematic-style"`; the motion `<style>` now
  uses that id, while the wrapper `<div>` keeps `hd-cinematic`. Four call sites
  updated (ensureStyle / removeStyle ×3).

---

## 4. Glass depth — real computed style (measured)

Captured from live `probeRealDom()` (frame = `--dsw-alias-bg-base`, sidebar =
`--dsw-specific-sidebar-fill`):

DEPTH_0_COMPUTED=frame rgba(250,251,253,0.68); sidebar rgba(246,247,250,0.56); root backdrop-filter none
DEPTH_50_COMPUTED=frame rgba(250,251,253,0.525); sidebar rgba(246,247,250,0.435)
DEPTH_100_COMPUTED=frame rgba(250,251,253,0.373); sidebar rgba(246,247,250,0.31); (no wallpaper)
  with wallpaper @100: frame rgba(250,251,253,0.30); sidebar rgba(246,247,250,0.243);
  root backdrop-filter blur(18px) saturate(1.16)

Each depth step produces a measurable delta on the real frame/sidebar, and the
backdrop-filter turns on as depth rises.

WALLPAPER_REAL_VISIBLE_PATH=PASS
REAL_SURFACE_MATCHES=PASS
REAL_COMPUTED_GLASS_CHANGE=PASS

---

## 5. MP4 bounded check

MP4_BOUNDED_CHECK=PASS (test-level; no codec re-verification here)
  - mediaType=video persisted + in engine payload (`engine_payload_includes_media_type`).
  - `.mp4` + `ftyp` magic + 96MB cap (`video_validation_accepts_mp4` / `_rejects_non_mp4`).
  - video DOM: muted/loop/autoplay/playsinline + controlled src
    (`tests/engine_apply.test.mjs` video case).
  - Range 206 serving (`parse_byte_range` + `serve_wallpaper` video/mp4 branch).
  Real H.264 playback remains a human-E2E item.

---

## 6. Verification

TYPECHECK=PASS
FRONTEND_BUILD=PASS
ENGINE_TESTS=PASS
ENGINE_APPLY_TESTS=PASS
HARNESS_COMPAT_TESTS=PASS
GUARD_TESTS=PASS
RUST_TESTS=PASS (132 passed, 0 failed)
CLIPPY=PASS
TAURI_BUILD=PASS (bundle: src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app)

---

## 7. Process safety / constraints

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0
COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

No IPC/config rewrite, no Harness React Core modification, no visual retuning.
Packaged-app test instances ran with isolated `HOME` (temp) and were stopped by
exact recorded PID (SIGTERM). No `pkill`/`killall`.

---

## 8. Changed files (this round)

- `src-tauri/src/engine.js` — fixed `hd-cinematic` id collision; added
  `probeRealDom()` real-DOM diagnostic (state + dom beacons already present).
- `src-tauri/src/lib.rs` — self-test now steps glassDepth 0→50→100.
- `tests/harness_compat.test.mjs` (new) — selector/token drift probe against the
  real bundled Harness source (skips when runtime not materialized).
- `tests/engine_apply.test.mjs` (prior round, retained) — fake-DOM apply path.
- `package.json` — `test:harness-compat` script.
- `docs/HARNESS_DESKTOP_V0.2_REAL_DOM_APPEARANCE_FIX.md` (this file).

Uncommitted prior-round artifacts preserved: `themes/deep-glass/`,
`docs/HARNESS_DESKTOP_V0.2_*_REPORT.md`, `runtime` (local gitignored symlink),
`appearance.html`, `src/appearance.ts`, `src-tauri/src/appearance.rs`,
`src-tauri/src/css_scope.rs`, `tests/engine.test.mjs`.

---

## 9. For the human reviewer

Launch the new `.app`, select Deep Glass + an image wallpaper + Surface
transparency 100. The wallpaper image must now be VISIBLE behind the Harness
surfaces, the sidebar/main surfaces read as translucent glass, the background
blurs at depth 100, and moving 0→50→100 visibly deepens the glass. If anything
still looks wrong, the startup log now contains a per-apply `hd-beacon://dom`
line with the real computed `body/#root/frame/sidebar` backgrounds and the
`#hd-wallpaper` element state to pinpoint the next layer.
