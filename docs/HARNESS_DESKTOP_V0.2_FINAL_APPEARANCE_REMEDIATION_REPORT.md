# DeepSeek Harness Desktop V0.2 — Final Appearance Remediation Report

WORKTREE=/Users/renjianxin/Harness-Desktop-v0.2-deep-glass-polish
BRANCH=v0.2/deep-glass-polish
BASE_HEAD=8ea5d8b7e3b2ca5ced9a53c8f3509b3b508d47c2

FINAL_STATUS=READY_FOR_FINAL_APPEARANCE_HUMAN_E2E

---

## 1. Diagnosis and fixes

ROOT_CAUSE_OF_WASHED_OUT_LOOK=
  1. Full-frame ambient gradient rendered OVER the wallpaper (the Deep Glass
     `asset` had white/light stops that paled the scene).
  2. A strong global readability scrim (default 0.45) darkening/fading the whole
     wallpaper.
  3. Translucent surface tints with no backdrop blur, so "transparency" read as a
     flat see-through rather than frosted glass.

Fix (no lower-alpha-only trick):
  - The ambient `asset` now renders ONLY when no wallpaper is active. With a
    wallpaper, the wallpaper is the background and stays vivid (no veil).
  - Global scrim is reduced by glass depth (0 → full, 100 → 0.35×).
  - A real, bounded `backdrop-filter: blur() saturate()` is applied to the Harness
    frame, plus a restrained edge highlight, so surfaces frost what is behind them.

REAL_BACKDROP_GLASS=IMPLEMENTED
  - `@supports`-gated `-webkit-backdrop-filter`/`backdrop-filter: blur(Npx) saturate(S)`
    on `#root` (the app frame), bounded (0–18px blur, 1.0–1.16 saturate), never
    animated, never applied to text. Unsupported WebViews degrade to the existing
    translucent surfaces.
  - Restrained edge highlight + depth shadow on stable readable surfaces
    (`#root [contenteditable]`, `#root textarea`, `#root pre`).

GLOBAL_SCRIM_CHANGE=REDUCED_BY_GLASS_DEPTH
  - `scrim = overlay × (1 - 0.65·depth/100)`. Depth 0 keeps the current overlay;
    depth 100 drops it to 0.35× so wallpaper color/detail stays alive. The
    readability overlay remains for arbitrary-wallpaper contrast.

GLASS_DEPTH_100=
  alpha 0.55 · backdrop blur 18px · saturate 1.16 · scrim ×0.35 · edge 0.26.
  Materially different from depth 0 (theme baseline, no blur, full scrim) and from
  the Human FAIL screenshot (which had only lower alpha, no blur, full scrim, and a
  pale full-frame veil).

CINEMATIC_MOTION=IMPLEMENTED
  - Engine-generated background-only drift on a media wrapper: `scale(1.00) → 1.04`
    + a slight diagonal pan, ~32s ease-in-out alternate. Never animates text,
    composer, dialogs, menus, or buttons. Compositor-friendly (transform only).

---

## 2. MP4 local video background (now mandatory)

MP4_BACKGROUND=IMPLEMENTED
MP4_VALIDATION=EXTENSION_MP4 + FTYP_MAGIC + 96MB_CAP
MP4_LOCAL_ONLY=TRUE
MP4_AUTOPLAY_MUTED_LOOP=TRUE
IMAGE_VIDEO_SWITCH=CLEAN_SWAP_WITH_CROSSFADE

Details:
  - `WallpaperSettings.media_type` ("image" | "video"), persisted, sanitized,
    default "image" (old `appearance.json` migrates unchanged).
  - Only `.mp4` accepted (no GIF/WebM/MOV/URL/transcoding/media library).
  - Local-only: bytes are validated, copied to the app wallpaper dir, served only
    through the existing `hd-wallpaper` scheme. Never uploaded; no arbitrary path
    exposed to page JS.
  - Served as `video/mp4` with a single-byte-Range (206) path so WebViews can
    stream/seek; images are served whole as before.
  - Rendered as a `<video muted loop autoplay playsinline>` behind the UI with
    `pointer-events:none`. `object-fit`/`object-position`/`opacity`/bounded
    `filter: blur()` follow the same wallpaper settings as images.
  - Switching image ↔ video pauses/hides the old element and swaps cleanly; a
    260ms crossfade runs on change (skipped at boot and under reduce-motion).
  - Playback failure falls back to the previous behavior (no crash; the app keeps
    working).

---

## 3. Readability / regression

READABILITY_GUARD=PASS
  - Composer/code pinned to the least-translucent `layer-3` surface; alpha floor
    0.15; readability overlay retained (reduced, not removed) for arbitrary
    wallpaper contrast; brand/text/accent tokens never scaled.

REDUCED_MOTION=RESPECTED
  - Engine skips theme motion + cinematic drift under `prefers-reduced-motion` and
    the Motion toggle; CSS `@media (prefers-reduced-motion: reduce)` guards remain
    as defense-in-depth. Crossfade is also skipped under reduce-motion.

STATIC_WALLPAPER_REGRESSION=PASS
  - PNG/JPG/JPEG/WebP accept, magic-byte validation, fit/position/opacity/blur/
    overlay/overlay-mode, local-only storage, remove/reset, persistence all pass.

OFFICIAL_OCEAN_STARTER_REGRESSION=PASS
  - All three resolve/apply unchanged at default depth (no blur, full scrim, no
    alpha change, no edge). One deliberate, documented behavior change: a theme's
    ambient `asset` no longer overlays an ACTIVE wallpaper (so the wallpaper stays
    vivid). Their palette/tokens/surfaces/components/motion/no-wallpaper look are
    unchanged.

---

## 4. Test / build results

TYPECHECK=PASS
FRONTEND_BUILD=PASS
ENGINE_TESTS=PASS
GUARD_TESTS=PASS
RUST_TESTS=PASS (130 passed, 0 failed)
CLIPPY=PASS
TAURI_BUILD=PASS (bundle: src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app)

Added/updated focused tests:
  - engine.test.mjs — depth→material mapping (alpha/blur/saturate/scrim/edge),
    scrim reduction, glass material (@supports blur + edge), cinematic drift
    (scale 1.04 + reduced-motion guard), plus existing alpha/token/motion tests.
  - appearance.rs — MP4 accepted, non-MP4 rejected, `ftyp` magic, media_type
    persist/sanitize, `mediaType` in engine payload, byte-range parsing.
  - css_scope.rs — Deep Glass components/motion still pass the CSS validator.

---

## 5. Self-review (slice-only)

1. Actual backdrop glass, not just lower opacity? YES — backdrop blur + saturation
   + edge highlight + depth-aware scrim, in addition to alpha.
2. Washed-out global veil reduced? YES — ambient no longer veils the wallpaper and
   scrim scales down with depth.
3. Depth 100 visibly deeper than the FAIL screenshot? YES (blur + saturation +
   vivid wallpaper + thinner tints).
4. Foreground readability safe? YES — layer-3 composer/code, alpha floor, retained
   overlay.
5. MP4 plays as a local background? YES — muted/loop/autoplay/playsinline over the
   local scheme with Range support.
6. Muted and local-only? YES — muted attribute; local-only copy+serve; no upload.
7. Reduced-motion respected? YES — engine gating + CSS guards.
8. Static wallpaper preserved? YES.
9. Harness Core touched? NO — surface layer only; no React tree, no upstream source.
10. Scope creep? NO — only the mandated glass material + MP4 + crossfade; no video
    library, no codecs, no transcoding.

---

## 6. Process safety

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0
COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

---

## 7. Changed files

Modified:
  - `appearance.html` — accept `video/mp4`.
  - `src/appearance.ts` — mediaType field, mp4 accept + size limits, i18n.
  - `src-tauri/src/appearance.rs` — media_type model, MP4 validation/store/serve
    (Range), payload, sanitize, set_wallpaper preservation, tests.
  - `src-tauri/src/engine.js` — cinematic media wrapper + video element, glass
    material (backdrop blur/edge), depth→material mapping, scrim reduction,
    crossfade, cinematic drift, ambient-only-without-wallpaper.
  - `src-tauri/src/css_scope.rs` — Deep Glass CSS validation test (prior round).
  - `src-tauri/src/lib.rs` — `set_glass_depth` command registration (prior round).
  - `tests/engine.test.mjs` — depth/material/motion/scrim/glass tests.
  - `themes/deep-glass/theme.json` — neutral depth asset, simplified components,
    28s ambient motion.

New (this round):
  - `docs/HARNESS_DESKTOP_V0.2_FINAL_APPEARANCE_REMEDIATION_REPORT.md`

Note: `runtime/` is a local-only symlink (gitignored build convenience, not
committed); `themes/deep-glass/README.md` and the prior-round report
`docs/HARNESS_DESKTOP_V0.2_DEEP_GLASS_REPORT.md` remain from the previous round.

---

## 8. For the human reviewer

Launch `src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app`, open
Appearance, select Deep Glass, add a vivid wallpaper, and set Glass ▸ Surface
transparency to 100. Confirm: the wallpaper reads as the scene (vivid, not pale),
surfaces frost it like glass (blur + edge highlight), composer/code stay readable,
the background drifts very slowly, Reduce Motion stops all movement, and selecting
a local MP4 plays it muted behind the UI.
