# DeepSeek Harness Desktop V0.2 — Foreground Readability / Composer Polish

WORKTREE=/Users/renjianxin/Harness-Desktop-v0.2-deep-glass-polish
BRANCH=v0.2/deep-glass-polish
BASE_HEAD=8ea5d8b7e3b2ca5ced9a53c8f3509b3b508d47c2

FINAL_STATUS=READY_FOR_FOREGROUND_POLISH_HUMAN_E2E

CONTROL_PATH_REOPENED=false

---

## 1. Composer (real DOM, measured)

COMPOSER_REAL_SELECTOR=#root textarea (uV2eYG_input); the painted surface is its
  ancestor card `.uV2eYG_card` whose background token is `--dsw-specific-input-major`
  (consumed by `dsh-client-ui-conversation/lib/client.js`). Real live probe walks
  up from the textarea to the first non-transparent container.

COMPOSER_BACKGROUND_BEFORE=rgb(255,255,255) pure white (light)
  (was `--dsw-specific-input-major: var(--dsw-static-neutral-bluish-00)`), and the
  prior theme `components` additionally forced `background: var(--dsw-alias-bg-layer-3)`
  = rgba(255,255,255,0.88).
COMPOSER_BACKGROUND_AFTER=rgba(244,246,249,0.72) neutral translucent (light);
  dark = rgba(36,36,40,0.72). Live probe confirmed: `uV2eYG_card` background
  `rgba(244,246,249,0.72)`.

COMPOSER_BLUR_BEFORE=12px backdrop-filter on the transparent textarea (no-op);
  container none.
COMPOSER_BLUR_AFTER=unchanged (bounded 12px, local; container remains clear glass).

COMPOSER_SHADOW_BEFORE=var(--dsw-shadow-lv2) (Harness default, subtle) + engine edge.
COMPOSER_SHADOW_AFTER=unchanged subtle shadow; edge highlight retained (1px inset
  top + 0 1px 6px depth).

COMPOSER_SIZE_CHANGE=border-radius 16px → 14px on the input surface (subtle);
  no height/padding/behavior change.

Net effect: the composer is no longer a bright pure-white SaaS card; it is a quiet
neutral translucent glass surface (72% alpha, slightly grayed RGB), still clearly
interactive and readable. Focused state uses `--dsw-alias-bg-layer-3` for a slightly
more stable working surface (idle = floating glass, focus = working glass).

---

## 2. Real Harness typography readability

PRIMARY_TEXT_CHANGE=none (primary label remains official near-black/near-white —
  high contrast).
SECONDARY_TEXT_CHANGE=strengthened (Deep-Glass scoped):
  light `--dsw-alias-label-secondary` rgb(97,102,107) → rgb(72,76,83);
  dark  rgb(207,211,214) → rgb(222,226,232).
  light `--dsw-alias-label-tertiary` rgb(129,133,140) → rgb(106,110,117);
  dark  rgb(173,178,184) → rgb(188,193,200).
SIDEBAR_TEXT_CHANGE=improved via the secondary/tertiary strengthening (session/
  workspace names use the secondary label); sidebar stays thin glass (0.16 @100).
MAIN_CONTENT_READABILITY_CHANGE=improved secondary/metadata contrast; no font-size
  or font-family change; no bold-everything.

LOCAL_READABILITY_PROTECTION=none added. Readability is recovered by raising
  secondary/tertiary foreground contrast (and the existing local sidebar/frame
  tints), NOT by adding any global veil.

GLOBAL_OVERLAY_ADDED=false
GLOBAL_BLUR_ADDED=false
(No full-screen scrim / opacity / backdrop-filter / saturation was introduced.)

---

## 3. Background clarity preserved

VIDEO_CLARITY_REGRESSION=none — no global filter; video still native playback;
  no cinematic transform (unchanged from Clear Glass PASS).
STATIC_IMAGE_CLARITY_REGRESSION=none — main canvas stays near-transparent
  (0.07 @100), open areas remain clear.

LIGHT_MODE=PASS — composer neutral translucent (not solid white); text high
  contrast; background clear.
DARK_MODE=PASS — composer neutral dark glass (rgba(36,36,40,0.72)); text readable;
  background clear.

---

## 4. Regression

OFFICIAL_REGRESSION=PASS — all changes are scoped to the Deep Glass theme
  (tokens/components); Official uses the engine's default surfaces unchanged.
OCEAN_REGRESSION=PASS — unchanged.
STARTER_REGRESSION=PASS — unchanged.
Static wallpaper / MP4 / opacity / blur / scrim / motion toggle / reduced-motion /
fallback / degraded preserved.

---

## 5. Verification

TYPECHECK=PASS
FRONTEND_BUILD=PASS
ENGINE_TESTS=PASS
ENGINE_APPLY_TESTS=PASS
HARNESS_COMPAT_TESTS=PASS
GUARD_TESTS=PASS
RUST_TESTS=PASS (132 passed, 0 failed)
CLIPPY=PASS
TAURI_BUILD=PASS (bundle: src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app)

Real-DOM metrics (packaged app, `probeRealDom()` / `hd-beacon://dom`):
  composer container `uV2eYG_card` background = rgba(244,246,249,0.72) (was white)
  #root backdrop-filter = none (no global blur reintroduced)
  main frame alpha @100 = 0.07 (scene clear)
  sidebar alpha @100 = 0.16 (thin glass)

---

## 6. Self review

1. Background clarity unchanged? YES (no global filter added).
2. Composer less dominant? YES (0.72 neutral vs pure white/0.88).
3. Composer still easy to use? YES (focus tier + readable text + local blur).
4. Main texts more readable? YES (secondary/tertiary contrast raised).
5. Avoided bolding/enlarging everything? YES (color/opacity only, no font-size/weight).
6. Avoided global overlay? YES.
7. Avoided global blur? YES.
8. Avoided stacked glass pollution? YES (no new layers; reused existing tints).
9. Light mode works? YES.
10. Dark mode works? YES.
11. Touched any functionality? NO.
12. Added any new feature? NO (no sliders/font controls/size settings/panels).

---

## 7. Process safety / constraints

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0
COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

No Harness React Core / runtime / bundled-dependency / IPC / config / control-plane
changes. Packaged-app test instances ran with isolated `HOME` (temp) and were
stopped by exact recorded PID (SIGTERM). No `pkill`/`killall`.

---

## 8. Changed files (this round)

- `themes/deep-glass/theme.json` — quiet composer via `--dsw-specific-input-major`
  (neutral translucent, light+dark), strengthened secondary/tertiary label tokens,
  composer radius 14px + focused `layer-3` tier; code/focus/selection unchanged.
- `src-tauri/src/engine.js` — `probeRealDom()` now also reports the composer
  textarea + its painted container (ancestor walk) for real-DOM evidence; probe
  truncation raised to 2600 chars.
- `tests/harness_compat.test.mjs` — added `--dsw-specific-input-major` to the
  engine-token drift list.
- `docs/HARNESS_DESKTOP_V0.2_FOREGROUND_POLISH_REPORT.md` (this file).

---

## 9. For the human reviewer

Launch the new `.app`, Deep Glass + image/MP4 wallpaper, Glass Depth 100, opacity
100 / blur 0 / scrim 0. The composer should now read as quiet neutral floating
glass rather than a bright white block; conversation/sidebar/metadata text should
be easier to read over the scene; the background remains clear and vivid. If this
is broadly acceptable, freeze Appearance and proceed to the V0.2 Release Candidate.
