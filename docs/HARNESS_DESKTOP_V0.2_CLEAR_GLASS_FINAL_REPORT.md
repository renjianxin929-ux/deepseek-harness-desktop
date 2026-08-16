# DeepSeek Harness Desktop V0.2 — Clear-Glass Final Architecture

WORKTREE=/Users/renjianxin/Harness-Desktop-v0.2-deep-glass-polish
BRANCH=v0.2/deep-glass-polish
BASE_HEAD=8ea5d8b7e3b2ca5ced9a53c8f3509b3b508d47c2

FINAL_STATUS=READY_FOR_CLEAR_GLASS_HUMAN_E2E

CONTROL_PATH_REOPENED=false

---

## 1. What was wrong (root design error)

ROOT_GLOBAL_BLUR_BEFORE=#root{backdrop-filter:blur(18px) saturate(1.16)}
  The engine applied a whole-viewport backdrop-filter to `#root`, turning the
  ENTIRE app into one frosted pane over the wallpaper. Higher depth increased
  that blur, so depth 100 = maximum fog. Combined with a ~30% milky frame tint,
  this produced exactly the human's "dirty / frosted / polluted" look.

ROOT_GLOBAL_BLUR_AFTER=none
  The `#root` backdrop-filter is removed. Live `probeRealDom()` at depth 100 now
  reports `#root` computed `backdrop-filter: none`.

FULL_VIEWPORT_GLASS_LAYER_COUNT_BEFORE=1  (#root full-screen frost + milky frame)
FULL_VIEWPORT_GLASS_LAYER_COUNT_AFTER=0   (no viewport-wide blur; frame near-transparent)

---

## 2. New architecture: clear scene + local glass

Real packaged-app computed styles (from `probeRealDom()` / `hd-beacon://dom`),
Deep Glass + image wallpaper + opacity 100 + blur 0 + scrim 0:

| Surface | depth 0 | depth 50 | depth 100 (+wallpaper) |
| --- | --- | --- | --- |
| main frame (`--dsw-alias-bg-base`) | rgba(250,251,253,0.45) | 0.27 | **0.07** (near transparent) |
| sidebar (`--dsw-specific-sidebar-fill`) | rgba(246,247,250,0.50) | 0.35 | **0.16** (thin glass) |
| #root backdrop-filter | none | none | **none** |

The main canvas is ~93% clear at depth 100 (scene shows through); the sidebar is
a thin clear glass; the scene is no longer frosted as a whole.

SIDEBAR_GLASS=thin clear glass — bg rgba(246,247,250,0.16) @100; no viewport blur
MAIN_CANVAS_GLASS=near transparent — bg rgba(250,251,253,0.07) @100; no blur
COMPOSER_GLASS=readable — `--dsw-alias-bg-layer-3` 0.88 × depth factor 0.93 ≈ 0.82 + LOCAL backdrop-filter blur(12px)
DIALOG_GLASS=readable — same `layer-3` tier ≈ 0.82

---

## 3. Depth semantics redefined

DEPTH_SEMANTICS_BEFORE=higher depth → more whole-screen blur (0→18px) + uniform alpha down (0.45 factor)
DEPTH_SEMANTICS_AFTER=higher depth → large surfaces become MORE transparent (frame 0.45→0.07), scene MORE visible, local composer blur BOUNDED at 12px (not depth-driven), composer/dialog stay readable (per-key factor 0.93)

Per-key depth response (in `engine.js` `depthKeyFactor`):
  main canvas 1.0→0.20 · sidebar 1.0→0.40 · cards 1.0→0.70 · elevated 1.0→0.80
  · panels 1.0→0.75 · composer/dialog 1.0→0.93 (readable)

Depth 100 is now the CLEAREST scene, not the blurriest.

---

## 4. Stacked alpha pollution

STACKED_ALPHA_POLLUTION_FIX=REMOVED
  - `#root` backdrop-filter removed (the single full-viewport frost layer).
  - `html`/`body`/`#root`/wrapper remain transparent.
  - Only the frame token (0.07) + sidebar token (0.16) + local composer/dialog
    tint + local composer blur remain — one meaningful glass layer per surface,
    no nested translucent layers compounding into gray/milky output.

---

## 5. Video / static image

VIDEO_NATIVE_CLARITY_PATH=PASS
  - mediaType=video gets NO cinematic pan/zoom (engine skips `hd-cinematic-style`).
  - No background-wide glass filter (root backdrop-filter none).
  - Native `<video muted loop autoplay playsinline>` with object-fit/position only.
VIDEO_EXTRA_TRANSFORM=DISABLED

STATIC_IMAGE_CLARITY=PASS
  - Main canvas 0.07 @100 keeps open areas sharp (scene detail recognizable).
  - The existing subtle cinematic pan/scale (1.00→1.04, 32s, image only) remains
    bounded and does not reduce clarity (no filter/blur on the image itself).

---

## 6. Regression

OFFICIAL_REGRESSION=PASS — Official uses the engine's default surfaces; at depth 0 (default) the per-key scaling is a no-op, so Official is pixel-identical. Depth control remains a global user control.
OCEAN_REGRESSION=PASS — unchanged tokens/surfaces/components/motion; no #root blur leak.
STARTER_REGRESSION=PASS — unchanged.
Static wallpaper / MP4 / opacity / blur / scrim / motion toggle / reduced-motion / fallback / degraded all preserved.

---

## 7. Verification

TYPECHECK=PASS
FRONTEND_BUILD=PASS
ENGINE_TESTS=PASS
ENGINE_APPLY_TESTS=PASS
HARNESS_COMPAT_TESTS=PASS
GUARD_TESTS=PASS
RUST_TESTS=PASS (132 passed, 0 failed)
CLIPPY=PASS
TAURI_BUILD=PASS (bundle: src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app)

Real-DOM acceptance (packaged app, `probeRealDom()`):
  #root backdrop-filter = none ✓
  main/frame backdrop-filter = none ✓
  frame alpha @100 = 0.07 (near transparent) ✓
  sidebar alpha @100 = 0.16 (thin glass) ✓
  wallpaper element present ✓

---

## 8. Self review

1. Removed viewport-wide blur? YES (`#root` backdrop-filter deleted).
2. Scene optically clear in open areas? YES (frame 0.07 @100).
3. Only local UI behaves like glass? YES (composer blur + edge; sidebar tint).
4. Depth 100 = clearer, not blurrier? YES (blur fixed 12px local; alpha drops).
5. Removed stacked alpha pollution? YES (single full-viewport frost layer removed).
6. Composer still readable? YES (layer-3 ≈0.82 + local 12px blur).
7. Sidebar still readable? YES (0.16 tint, text is opaque `--dsw-alias-label-primary`).
8. Video free of extra cinematic transforms? YES.
9. Avoided new features? YES — no new preset/slider/media/animation/system.
10. Relied on REAL computed styles? YES (live `probeRealDom()` beacon, not fake DOM).

---

## 9. Process safety / constraints

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0
COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

No Harness React Core, IPC, config, wallpaper protocol, MP4 protocol, or control-plane
changes. Packaged-app test instances ran with isolated `HOME` (temp) and were
stopped by exact recorded PID (SIGTERM). No `pkill`/`killall`.

---

## 10. Changed files (this round)

- `src-tauri/src/engine.js` — removed `#root` global blur; local composer blur
  (bounded 12px); per-key `depthKeyFactor` clear-glass depth semantics; lower
  alpha floor (0.02) so the main canvas can go near-transparent; cinematic
  pan/scale now image-only (video excluded).
- `themes/deep-glass/theme.json` — clear-glass surface hierarchy (main canvas/
  sidebar much thinner; composer/dialog readable).
- `tests/engine.test.mjs` — rewritten for clear-glass depth semantics + no-global-blur.
- `tests/engine_apply.test.mjs` — asserts no `#root` blur + composer-only blur +
  video has no cinematic style.
- `tests/harness_compat.test.mjs` — asserts composer surface is a real
  contenteditable/textarea.
- `docs/HARNESS_DESKTOP_V0.2_CLEAR_GLASS_FINAL_REPORT.md` (this file).

---

## 11. For the human reviewer

Launch the new `.app`, select Deep Glass + an image wallpaper, set opacity 100 /
blur off / scrim 0 / Surface transparency 100. The wallpaper scene must now be
optically CLEAR in the open workspace (no full-screen frost), the sidebar reads
as thin glass, the composer reads as readable clear glass, and raising depth makes
the scene CLEARER (not blurrier). Human remains the final visual authority.
