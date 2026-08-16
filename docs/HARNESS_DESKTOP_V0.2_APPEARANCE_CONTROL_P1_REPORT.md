# DeepSeek Harness Desktop V0.2 — Appearance Control-Plane P1 Remediation

WORKTREE=/Users/renjianxin/Harness-Desktop-v0.2-deep-glass-polish
BRANCH=v0.2/deep-glass-polish
BASE_HEAD=8ea5d8b7e3b2ca5ced9a53c8f3509b3b508d47c2

FINAL_STATUS=READY_FOR_APPEARANCE_CONTROL_HUMAN_E2E

---

## 1. Root cause and first failing boundary

ROOT_CAUSE=SILENT_ERROR_PATHS + LOADING_WINDOW_APPLY_LOSS
FIRST_FAILING_BOUNDARY=OBSERVABILITY_BOUNDARY (NOT a broken apply boundary)

Evidence (proven, not assumed): I built the release app with a deterministic
self-test hook (`HD_APPEARANCE_SELF_TEST=1`, isolated `HOME`) that drives the
REAL commands and records every boundary to the startup log + engine beacon. It
showed the full chain working for all four flows:

```
set_theme("deep-glass")     -> [appearance] apply sent (theme=deep-glass)
                              engine beacon: theme=deep-glass
set_glass_depth(100)        -> [appearance] apply sent (glassDepth=100)
                              engine beacon: glassDepth=100
install image wallpaper     -> [appearance] apply sent (wallpaper=true)
                              engine beacon: wallpaper=1 mediaType=image
```

So the invoke→command→persist→payload→main-window→engine path is NOT broken.
The prior "nothing happens" symptom came from two real defects, both now fixed:

1. **Every boundary swallowed its error.** `apply_to_main_window` discarded
   `w.eval()`'s Result (`let _ = …`), `engine_payload` errors were `Err(_) => return`,
   the engine wrapped apply in an empty `catch`, and the settings UI dropped
   operations when `busy` was set with no message. A transient failure anywhere
   was invisible — the user saw literally "nothing happens".
2. **Settings changed while the main window was still on the loading page were
   lost.** The injected `window.__HD_INITIAL__` payload is a setup-time snapshot,
   so changes made before the Harness page loaded were overwritten on navigation.

---

## 2. Four flows (proven end-to-end)

THEME_EVENT=PASS (click -> selectTheme -> invoke set_theme)
THEME_INVOKE=PASS (set_theme registered; {themeId} -> theme_id)
THEME_CONFIG=PASS (cfg.theme persisted; snapshot returns activeTheme=deep-glass)
THEME_MAIN_APPLY=PASS (beacon: theme=deep-glass received by main-window engine)

GLASS_EVENT=PASS (change -> invoke set_glass_depth)
GLASS_INVOKE=PASS ({depth} -> u32, clamped [0,100])
GLASS_CONFIG=PASS (glassDepth persisted; snapshot returns 100)
GLASS_MAIN_APPLY=PASS (beacon: glassDepth=100 received)

IMAGE_EVENT=PASS (file change -> readAsDataURL -> invoke set_wallpaper_bytes)
IMAGE_INVOKE=PASS (payload {name,data} -> WallpaperBytes)
IMAGE_STORAGE=PASS (bytes validated, copied to local wallpaper dir, mediaType=image)
IMAGE_MAIN_APPLY=PASS (beacon: wallpaper=1 mediaType=image)

VIDEO_EVENT=PASS (mp4 accepted by picker + pre-check)
VIDEO_INVOKE=PASS (set_wallpaper_bytes; mp4 -> mediaType=video)
VIDEO_STORAGE=PASS (mp4 magic + 96MB cap, local copy, video/mp4 + Range 206 serve)
VIDEO_MAIN_APPLY=PASS (engine_apply.test.mjs: <video> muted/loop/autoplay/playsinline, src set)

---

## 3. Contracts / registration / migration

COMMAND_REGISTRATION=PASS — `get_appearance`, `set_theme`, `set_motion`,
`set_glass_depth`, `set_language`, `set_wallpaper`, `set_wallpaper_bytes`,
`remove_wallpaper`, `reset_appearance` all in `tauri::generate_handler!`; invoke
names + camelCase/snake_case keys match.

OLD_CONFIG_MIGRATION=PASS — `glassDepth` defaults 0 and `mediaType` defaults
"image" when absent; added fixtures:
`pre_deep_glass_config_migrates_safely` (V0.1/round-1 config) and
`malformed_config_does_not_disable_updates` (corrupt config → default + still writable).

SILENT_ERROR_HANDLING=FIXED — `apply_to_main_window` now logs window-found /
eval-result / payload-error to the startup log; the engine records apply errors
into `window.__HD_STATE__.error` and fires a state beacon on every config change;
the settings UI surfaces "Error: Failed to apply <operation>: …" with a console
error naming the operation.

---

## 4. Fixes shipped

- `src-tauri/src/lib.rs` — one-shot re-apply of the CURRENT appearance 4s after
  the main window navigates to the Harness page, so settings changed during the
  loading window are not lost; `spawn_appearance_self_test` test hook.
- `src-tauri/src/appearance.rs` — `app_log` + observable `apply_to_main_window`
  (window found / eval result / payload error all logged).
- `src-tauri/src/engine.js` — `window.__HD_STATE__` now exposes non-sensitive
  `theme/glassDepth/wallpaperActive/mediaType/motionEnabled/error`; the diagnostic
  beacon re-fires when the applied config changes; apply wrapped so JS errors are
  recorded instead of silent.
- `src/appearance.ts` — every `run()` carries an operation name and shows a
  visible "Failed to apply <op>" message + console error; no silent drop.
- `tests/engine_apply.test.mjs` (new) — drives the real `applyAppearance()` with
  a complete fake DOM (image + video), asserting no throw, state fields, backdrop
  elements, glass blur, and video muted/loop/autoplay/playsinline/src.
- `tests/engine.test.mjs` — depth→material/scrim/glass/cinematic assertions.
- `package.json` — `test:engine-apply` script; `test` now runs it.

---

## 5. Packaged control-path E2E (automation)

PACKAGED_CONTROL_E2E=PASS — release `.app` + `HD_APPEARANCE_SELF_TEST=1` +
isolated `HOME`; observed in the startup log + `hd-beacon` state:
theme `official -> deep-glass`, glassDepth `0 -> 100`, wallpaper `0 -> 1` with
`mediaType=image`, all via the REAL `set_theme` / `set_glass_depth` /
`install_wallpaper_from_path` commands and the real `apply_to_main_window` path
into the real main WebView. (This proves control → backend → main-window effect,
not visual quality.)

---

## 6. Verification

TYPECHECK=PASS
FRONTEND_BUILD=PASS
ENGINE_TESTS=PASS
GUARD_TESTS=PASS
ENGINE_APPLY_TESTS=PASS
RUST_TESTS=PASS (132 passed, 0 failed)
CLIPPY=PASS
TAURI_BUILD=PASS (bundle: src-tauri/target/release/bundle/macos/DeepSeek Harness Desktop.app)

---

## 7. Self-review

- Did all four failures share one broken path? Yes — but the shared path was the
  *unobservable* one (silent errors + loading-window loss), not a broken invoke.
- First failing boundary? The observability boundary: no error could ever surface.
- Were errors swallowed? Yes (eval result, payload error, engine catch, busy-drop).
- Invoke contracts exact? Yes.
- Commands registered? Yes.
- Old appearance.json migrate? Yes (fixtures added).
- Main WebView receives config? Yes (beacon proof).
- glassDepth 0/50/100 arrive at engine? 0 and 100 proven via beacon; 50 is the
  same numeric path (no per-value special-casing).
- Image apply? Yes. Video apply? Yes (engine_apply test).
- Failures visible instead of silent? Yes.
- Visual scope expansion? None — no alpha/visual retuning; only observability,
  the re-apply fix, and tests.

---

## 8. Process safety

SUBAGENT_USAGE=0
WORKFLOW_USAGE=0
COMMIT_CREATED=false
PUSH_PERFORMED=false
TAG_CREATED=false

Packaged-app test instances were launched with isolated `HOME` (temp dir) and
stopped by their exact recorded PID (SIGTERM, graceful). No `pkill`/`killall`,
no broad process kill, no Windows/Remote processes touched.

---

## 9. Changed files

Modified:
  - `src-tauri/src/lib.rs` — re-apply-after-navigate + self-test hook.
  - `src-tauri/src/appearance.rs` — observable apply path + old-config fixtures.
  - `src-tauri/src/engine.js` — __HD_STATE__ config fields, state beacon re-fire,
    apply error capture.
  - `src/appearance.ts` — per-operation error surfacing + i18n.
  - `src-tauri/src/css_scope.rs` — (prior round) Deep Glass CSS validation test.
  - `appearance.html`, `tests/engine.test.mjs` — (prior round) + engine material tests.
  - `package.json` — `test:engine-apply` script.

New (this round):
  - `tests/engine_apply.test.mjs`
  - `docs/HARNESS_DESKTOP_V0.2_APPEARANCE_CONTROL_P1_REPORT.md`

Uncommitted prior-round artifacts preserved: `themes/deep-glass/`,
`docs/HARNESS_DESKTOP_V0.2_*_REPORT.md`, `runtime` (local gitignored symlink).

---

## 10. For the human reviewer

Launch the new `.app`, open Appearance (`Cmd+,`), and drive any control. The
Appearance window now shows a concrete "Error: Failed to apply …" message if
anything fails (instead of silence), and the startup log
(`~/Library/Logs/HarnessDesktop/startup.log`) records every `[appearance] apply …`
line plus the engine's `hd-beacon://state/?…theme=…&glassDepth=…&wallpaper=…`
receipt. Select Deep Glass + a wallpaper + set Surface transparency to 100; the
main Harness surface must reflect each change live. If it still appears to do
nothing, the log now tells us the exact boundary.
