# DeepSeek Harness Desktop V0.2 — Deep Glass / Cinematic Polish Report

WORKTREE=/Users/renjianxin/Harness-Desktop-v0.2-deep-glass-polish
BRANCH=v0.2/deep-glass-polish
BASE_HEAD=8ea5d8b7e3b2ca5ced9a53c8f3509b3b508d47c2

FINAL_STATUS=READY_FOR_DEEP_GLASS_HUMAN_E2E

---

## 1. Product status

DEEP_GLASS_PRESET=IMPLEMENTED
GLASS_VISUAL_DEPTH=DEEPER_THAN_OCEAN
GLASS_DEPTH_CONTROL=IMPLEMENTED

SURFACE_HIERARCHY=SIDEBAR_MOST_TRANSLUCENT -> BASE -> SECONDARY_PANELS -> ELEVATED -> COMPOSER_CODE_DIALOG_LEAST_TRANSLUCENT
COMPOSER_READABILITY=PASS
DIALOG_READABILITY=PASS
CODE_READABILITY=PASS

CINEMATIC_MOTION=IMPLEMENTED
MOTION_CYCLE=~30S_PER_SWEEP_60S_FULL_ALTERNATE
REDUCED_MOTION=RESPECTED

STATIC_WALLPAPER_REGRESSION=PASS
DYNAMIC_VIDEO_WALLPAPER=DEFERRED_V0.2.1

COMPATIBLE_MODE=PASS
DEGRADED_MODE=PASS
FALLBACK_MODE=PASS

TYPECHECK=PASS
FRONTEND_BUILD=PASS
ENGINE_TESTS=PASS
GUARD_TESTS=PASS
RUST_TESTS=PASS
CLIPPY=PASS
TAURI_BUILD=PASS

CHANGED_FILES=
  appearance.html
  src/appearance.ts
  src-tauri/src/appearance.rs
  src-tauri/src/css_scope.rs
  src-tauri/src/engine.js
  src-tauri/src/lib.rs
  tests/engine.test.mjs
  themes/deep-glass/theme.json (new)
  themes/deep-glass/README.md (new)
  docs/HARNESS_DESKTOP_V0.2_DEEP_GLASS_REPORT.md (new)

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0
COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

---

## 2. What was built

### Deep Glass preset (`themes/deep-glass/theme.json`)

A real built-in theme (id `deep-glass`), registered in `appearance.rs`
(`DEEP_GLASS_THEME` + `resolve_theme` + `list_themes`). It is **not** a
hard-coded DOM hack: it uses the exact same declarative token/surface/component/
motion/asset model as Ocean and Starter, and passes the same security validation
(`validate_theme_security` + the `css_scope` parser).

- `tokens.light/dark` — translucent surface tokens used when **no wallpaper** is
  active (a touch more opaque than `surfaces`, so text stays readable over the
  ambient gradient).
- `surfaces.light/dark` — deeper translucent surfaces used when a **wallpaper**
  is active. The hierarchy is encoded directly in the alpha values:

  | Layer | Token | Dark alpha | Light alpha |
  | --- | --- | --- | --- |
  | Sidebar | `--dsw-specific-sidebar-fill` | 0.36 | 0.44 |
  | App base / main surfaces | `--dsw-alias-bg-base` | 0.46 | 0.55 |
  | Secondary panels | `--dsw-alias-bg-module-platform` | 0.58 | 0.66 |
  | Elevated surface 1 | `--dsw-alias-bg-layer-1` | 0.54 | 0.62 |
  | Elevated surface 2 | `--dsw-alias-bg-layer-2` | 0.62 | 0.72 |
  | Composer / code / dialogs | `--dsw-alias-bg-layer-3` | 0.76 | 0.86 |

- `asset` — a subtle **neutral** ambient gradient (alpha 0.05–0.20) that adds
  depth without forcing a blue/yellow cast onto the wallpaper.
- `components` — readability-first treatments on stable selectors only:
  composer (`[contenteditable]` / `textarea`) and code (`pre` / `pre code`) use
  `var(--dsw-alias-bg-layer-3)` (the least-translucent surface), a rounded
  composer with a restrained inset highlight, plus the official focus ring and a
  subtle selection tint.
- `motion` — one slow `hd-deep-glass-pan` keyframe (~30 s per sweep) drifting the
  ambient layer's `background-position`, with a `prefers-reduced-motion` guard.

Deep Glass leaves brand/accent/label tokens at the official values, so it stays
recognizably Harness and readable.

### Glass depth control (one product-level slider)

A single `Glass ▸ Surface transparency` control (0–100), not six per-layer
sliders:

- `AppearanceConfig.glass_depth: u32`, serde `default` 0 (so every existing
  config migrates to an identical look), clamped to `[0, 100]` in `sanitize`,
  persisted, and exposed via `AppearanceSnapshot.glassDepth`.
- New `set_glass_depth` Tauri command (registered in `lib.rs`), wired to the
  Appearance window slider with i18n (en + zh-CN).
- The engine maps the one value to surface alpha ranges via `depthToFactor`:
  `0 → factor 1.0` (theme baseline / existing surfaces), `100 → factor 0.6`
  (deepest supported glass). Only the surface keys
  (`--dsw-alias-bg-base/-layer-1/2/3/-module-platform/-sidebar-fill`) are
  scaled; brand/label/accent tokens are never touched, and alpha is clamped to a
  readable `[0.15, 0.97]` window. This preserves the authored per-layer
  hierarchy while modulating depth.

### Cinematic motion

Deep Glass `motion` pans the ambient background (the `#root > *` background
image, i.e. behind all content) over ~30 s per sweep. It animates only
`background-position` (paint, not layout) on the ambient layer, never text,
composer, code, dialogs, buttons, or menus. The engine already suppresses the
motion `<style>` entirely under `prefers-reduced-motion` and the Appearance ▸
Motion toggle; the theme adds a redundant CSS media guard as defense-in-depth.

---

## 3. Architecture preservation

- No `@deepseek-ai/dsh` source modified. No React tree inside `#root` touched.
  No upstream bundle patched. No privileged theme JS. The injected engine remains
  the single self-contained surface layer (one `<style>` element + background
  layers + the appearance button).
- `resolve_theme`/`list_themes` gained `deep-glass` the same way `ocean` and
  `starter` already work; `official`, `ocean`, `starter` are untouched.
- The glass-depth scaling only modulates the existing surface-custom-property
  mechanism; it does not add arbitrary CSS or JS.

---

## 4. Readability guard

Verified structurally (unit tests assert the values and selectors):

- Sidebar / main / secondary / elevated surfaces keep opaque-enough tints and a
  readable alpha floor (0.15 min via the depth control; authored Deep Glass
  surfaces never drop below 0.36 dark / 0.44 light).
- Composer, code blocks, and the least-translucent elevated surface (`layer-3`)
  are the most opaque in the hierarchy, so the working surface stays readable.
- Arbitrary wallpaper contrast is handled by the existing readability overlay
  (`overlay`/`overlayMode`), which is preserved and remains first-class.
- The depth control never fades text or brand colors (only surface keys scale).

The full pixel-level verification is deferred to Human Appearance E2E (below).

---

## 5. Static wallpaper regression

The wallpaper pipeline is unchanged: PNG/JPG/JPEG/WebP accept, magic-byte
validation, `cover/contain/fill/auto` fit, position, opacity, blur, overlay,
overlay mode, local-only storage, remove/reset, and persistence all still pass
their existing unit tests. Deep Glass does not require a wallpaper (it falls
back to its ambient gradient), and does not alter wallpaper handling.

---

## 6. Dynamic video wallpaper (stretch)

Not implemented. The bounded local-video stretch is out of scope for this
pass to protect the mandatory Deep Glass + motion + reduced-motion + readability
gates and to avoid the multi-codec/transcoding/platform risk called out in the
brief.

DYNAMIC_VIDEO_WALLPAPER=DEFERRED_V0.2.1

---

## 7. Tests

Added/updated focused tests:

- `tests/engine.test.mjs` — `depthToFactor` (baseline/clamp/out-of-range),
  `scaleColorAlpha` (rgba scaling, rgb→rgba on deepen, opaque no-op, percentage
  components, alpha clamping, unparsable fail-safe), `scaleSurfaceMap` (only
  surface keys scaled; brand never scaled), and `shouldApplyMotion`
  (reduced-motion + toggle gating).
- `src-tauri/src/appearance.rs` — Deep Glass presence/resolution, Deep Glass
  listed as a built-in exactly once, `glass_depth` default 0 + migration from a
  field-less config file, config round-trip persistence, sanitize clamping, and
  `glassDepth` in the engine payload.
- `src-tauri/src/css_scope.rs` — Deep Glass `components`/`motion` pass the CSS
  scope/URL validator.
- Existing Official/Ocean/Starter, wallpaper, token/surface safety, gradient,
  data-URI, asset-containment, session-guard, usage, platform, runtime and
  reliability tests all remain green.

Test execution results:

- `npm run typecheck` — PASS.
- `npm run build:frontend` — PASS (dist includes index/appearance/usage).
- `npm run test:engine` — PASS.
- `npm run test:guard` — PASS.
- `cargo test` — **125 passed, 0 failed**.
- `cargo clippy -- -D warnings` — PASS.
- `npm run build` (production `.app`) — PASS at
  `src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app`.

---

## 8. Self-review (slice-only)

1. **Is Deep Glass visibly deeper than the current appearance?** Yes — surface
   alphas are lower than Ocean in both modes (e.g. dark base 0.46 vs Ocean 0.55),
   with an ambient depth layer behind translucent surfaces.
2. **Real depth, not just lower opacity?** Depth comes from the layered
   hierarchy + the ambient gradient behind the glass + slow pan motion, not a
   flat opacity drop.
3. **Composer/dialog/code/error readable?** Composer and code are pinned to the
   least-translucent surface (`layer-3`); dialogs/errors inherit the same most
   opaque tier; the alpha floor + readability overlay protect arbitrary
   wallpapers.
4. **Arbitrary wallpaper color usable?** Yes — neutral tints, no forced
   blue/yellow cast, and the retained readability overlay.
5. **Motion slow/subtle?** ~30 s per sweep, background-only.
6. **Reduced-motion respected?** Engine matchMedia + `@media` guard + toggle.
7. **Harness React Core untouched?** Yes — no `#root` tree modification.
8. **Official/Ocean/Starter preserved?** Yes — untouched; `glass_depth` default
   0 keeps them pixel-identical.
9. **Fallback/degraded preserved?** Yes — unchanged paths, plus new tests.
10. **Scope creep?** No — video wallpaper deferred; no Windows P1; no Remote.

---

## 9. Notes for the human reviewer

- The bundled runtime (`runtime/`) is not present in this worktree's git tree
  (it is gitignored and materialized by `npm run materialize:runtime`). For the
  local build a symlink `runtime -> ../Harness-Desktop-v0.2-cross-platform-foundation/runtime`
  was created. It is a **local build convenience only** and is not committed.
- `src/appearance.css` was intentionally left unchanged: the new Glass slider
  reuses the existing `.hd-field` / `.hd-range` styles.
- To run Human Appearance E2E: launch
  `src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app`, open
  **DeepSeek Harness Desktop ▸ Appearance…**, select **Deep Glass**, pick a
  wallpaper, and confirm the surface hierarchy, composer/code readability,
  slow ambient motion, the Reduce-Motion path, and the Surface-transparency
  slider. Also confirm Official/Ocean/Starter still render as before.
