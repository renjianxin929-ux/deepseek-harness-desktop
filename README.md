# Harness Desktop

A native **macOS** desktop shell for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness), plus a safe, user-customizable **appearance system**.

> **Keep Harness official. Make it yours.**

Harness Desktop launches your existing `@deepseek-ai/dsh` runtime locally (`127.0.0.1`, dynamic port) and wraps the **official** DeepSeek Harness Web UI in a native window — without modifying Harness Core. On top of that, it adds a *surface-layer* appearance system: user-selectable themes, a DIY starter theme, a local wallpaper, and optional lightweight motion.

---

## ⚠️ Unofficial project

Harness Desktop is an **independent, unofficial** project. It is not affiliated with, endorsed by, or associated with DeepSeek. The DeepSeek Harness software, its Web UI, and the DeepSeek whale mark remain the property of their respective owners.

- This project **reuses** the official Harness Web UI over its local HTTP server; it does not bundle, fork, or modify Harness Core.
- The Ocean showcase uses the official DeepSeek whale silhouette (unaltered) as a faint decorative element.
- MIT-licensed code in this repository is this project's own; upstream Harness is distributed under its own license.

---

## Project purpose

DeepSeek Harness already ships a capable Web UI (`dsh web`). Harness Desktop gives it a first-class native window and a clean way to *personalize the look* without ever touching Harness internals, credentials, sessions, or Agent behavior.

## Features

- **Native macOS window** around the official Harness Web UI (local-only serving, dynamic port, readiness handling, clean shutdown, child-process cleanup).
- **Appearance system** — surface-layer themes that sit *on top of* the official UI:
  - **Official** — the untouched Harness appearance.
  - **Ocean** — a polished, restrained ocean-inspired showcase.
  - **Starter (Sunset)** — a documented template for your own theme.
- **DIY themes** — declarative JSON themes (tokens + constrained CSS + assets). No Harness Core changes, no forking, no arbitrary JavaScript.
- **Local wallpaper** — PNG / JPG / JPEG / WebP, with fit, position, opacity, blur, and readability-overlay controls. Persists across restart; stored locally and never uploaded.
- **Optional motion** — lightweight, decorative, respects `prefers-reduced-motion`.
- **Safe fallback** — if an upstream Harness UI change makes customization unsafe, the appearance layer degrades or falls back without breaking Harness.

---

## Architecture overview

```
┌────────────────────────────────────────────────────────────┐
│ Tauri (Rust)                                                │
│  • resolves + launches `dsh web` on 127.0.0.1:<dynamic>     │
│  • manages the child process group + shutdown               │
│  • owns appearance state (persisted to app config dir)      │
│  • serves the local wallpaper via `hd-wallpaper://`         │
│  • exposes narrow IPC commands to the settings window       │
└───────────────┬──────────────────────────────┬─────────────┘
                │ navigates                     │ IPC (own origin)
        ┌───────▼────────┐              ┌───────▼──────────────┐
        │ main window     │              │ settings window      │
        │ (Harness UI)    │              │ (appearance.html)    │
        │ + injected      │              │ theme/wallpaper/     │
        │   engine.js     │              │ motion controls      │
        └─────────────────┘              └──────────────────────┘
```

### Key design decisions

1. **The Harness Web UI is never modified.** The WebView navigates to the official UI exactly as `dsh web` serves it. Appearance is injected by a self-contained `engine.js` (an *initialization script*) that only adds a `<style>` element and a few `pointer-events:none` background layers to `<body>`. It never touches the React tree inside `#root`.
2. **Themes are declarative, not code.** A theme is a JSON object with token overrides (the official `--dsw-*` CSS custom-property contract), optional scoped component CSS, optional motion CSS, and an optional ambient asset. There is **no** theme JavaScript, no shell access, and no filesystem/credential access.
3. **Wallpaper stays local.** Selected images are validated and copied into the app config directory (`~/Library/Application Support/…/wallpaper/`), never into the repository, never uploaded, never logged as image content.
4. **Compatibility is a first-class state.** On load the engine verifies (a) that `#root` mounted and (b) that the core theme token contract still resolves. It then applies one of three states — `compatible`, `degraded` (wallpaper only), or `fallback` (nothing) — so an upstream change cannot turn an appearance mismatch into a broken app.

---

## Requirements

- **macOS** (11+)
- **Node.js** (≥ 20) and npm
- **DeepSeek Harness `0.1.0-rc.6`** (`@deepseek-ai/dsh`)
- **Python is NOT required**

> V0.1 contains an experimental automatic Harness bootstrap path, but a clean first-install bootstrap is **NOT guaranteed**. Install DeepSeek Harness `0.1.0-rc.6` yourself before relying on Harness Desktop.

---

## Installation / build

Prerequisites (macOS 11+):

- [Node.js](https://nodejs.org) ≥ 20 and npm
- [Rust](https://rustup.rs) ≥ 1.77 (`rustup` + `cargo`)
- A working DeepSeek Harness install (`@deepseek-ai/dsh@0.1.0-rc.6`) — see [Usage](#usage)

```bash
npm install
npm run build:frontend   # builds the loading page + settings window (Vite)
cargo check              # inside src-tauri/ — type-checks the Rust backend
npm run tauri build      # production .app bundle (outputs to src-tauri/target/release/bundle)
```

Development loop:

```bash
npm run tauri dev        # runs Vite + cargo run, launches the app
```

### Regression checks

```bash
npm run typecheck                        # tsc --noEmit
npm run build:frontend                   # Vite production build
(cd src-tauri && cargo test)             # Rust unit tests
(cd src-tauri && cargo clippy -- -D warnings)   # lints (recommended)
npm run tauri build                      # production Tauri build
```

---

## macOS usage

1. Launch **Harness Desktop**. The loading screen resolves your runtime, pins it to Harness `0.1.0-rc.6`, starts `dsh web --host 127.0.0.1 --port 0`, waits for readiness, then opens the official Harness UI.
2. To change the look, either:
   - click the small **gear button** in the bottom-right corner of the Harness window, or
   - choose **Harness Desktop ▸ Appearance…** from the menu bar (`⌘,`).

The Appearance window lets you switch themes, pick a wallpaper and tune it, toggle motion, and reset to Official. Changes apply immediately and persist.

The Appearance window also has a **Language / 语言** selector with three options — **System / 跟随系统** (the default; a Chinese system locale shows Simplified Chinese, otherwise English), **简体中文**, and **English**. Your choice persists across restarts; choosing **System** keeps following the OS locale. This only localizes the Appearance settings window, never the official Harness UI.

> **Note on `@deepseek-ai/dsh` version pinning.** Harness Desktop refuses to start any Harness version other than `0.1.0-rc.6` (no silent upgrade or downgrade). This keeps the desktop shell honest about what it was validated against. See [Compatibility & fallback](#compatibility--fallback).

---

## DIY themes

A theme is a folder with a `theme.json` and (optionally) an `assets/` directory. Copy the [`themes/starter`](themes/starter) folder, edit a few values, and drop it into:

```
~/Library/Application Support/com.deepseek.harnessdesktop/themes/<your-theme-id>/theme.json
```

A theme can change:

| Aspect | Where | Example |
| --- | --- | --- |
| Colors / tokens | `tokens.light` / `tokens.dark` | `"--dsw-alias-brand-primary": "rgb(25,108,150)"` |
| Component styling | `components` (scoped CSS) | round the composer, tint the selection |
| Decorative assets | `asset` | a gradient or `assets/whale.svg` |
| Motion | `motion` (CSS keyframes) | a slow ambient drift |
| Wallpaper surfaces | `surfaces` (translucent tokens) | glass effect when wallpaper is active |

See **[themes/starter/README.md](themes/starter/README.md)** for a full, step-by-step walkthrough (a frontend-capable developer can produce a first theme in ~10–20 minutes).

**Safety:** themes are untrusted appearance content. The engine only accepts custom-property token overrides and injects `components`/`motion` as *CSS text* (never JavaScript). A theme cannot reach Harness credentials, sessions, the shell, or Agent privileges.

### Theme CSS & asset safety

`components` and `motion` are validated by a real CSS parser before they ever reach the WebView, and any violation rejects the whole theme (fail-closed):

- **Selector scope** — every selector must begin with `#root` and may only descend (`>` child or space descendant). `html`, `body`, `:root`, sibling/column combinators, `:has()`, and anything that could escape `#root` are rejected. `:is()`, `:where()`, `:not()`, and `:nth-child(... of …)` are validated recursively.
- **At-rules** — only `@media` and `@supports` are allowed (inner rules validated recursively). `@keyframes` is allowed only with an `hd-` name prefix; its selectors are `from` / `to` / percentages. `@import`, `@font-face`, and unknown at-rules are rejected.
- **No URL-bearing CSS** — `url(...)` (remote fonts, background URLs, any protocol) is rejected.

Decorative `asset` values are confined to the theme's own `assets/` directory:

- Allowed types: **SVG / PNG / JPG / JPEG / WebP**, each ≤ **512 KB**, with magic-byte validation for PNG/JPEG/WebP.
- `assets/<file>` references are resolved via canonical-path containment — `../`, absolute paths, backslashes, symlinks, and other-theme access are rejected. Files that do not exist are rejected.
- Alternatively, `asset` may be a CSS gradient or a `data:image/*;base64,…` URI (image MIME types only, size-capped); `file:`, `http:`, `https:`, and protocol-relative URLs are rejected.

---

## Wallpaper

- Click **Choose image…** and select a **PNG**, **JPG/JPEG**, or **WebP** file.
- Tune **fit** (cover/contain/fill/original), **position**, **opacity**, **blur**, and the **readability overlay**.
- The overlay preserves text contrast over light or busy images; choose **Auto** (follows the active Harness theme), **Dark**, or **Light**.
- **Remove wallpaper** clears it. Wallpapers persist across restarts and are stored only in the app config directory.

Unsupported, renamed, oversized, or corrupt files are rejected before anything is written.

---

## Motion & reduced motion

Motion is **optional** (toggle in the Appearance window) and decorative only:

- animations are CSS-only, GPU-light, and never block clicks, text input, scrolling, or navigation;
- the system **Reduce Motion** setting (`prefers-reduced-motion`) disables theme motion automatically and is always respected;
- no decoration intercepts pointer interaction (all injected layers are `pointer-events: none`).

---

## Compatibility & fallback

DeepSeek Harness is upstream software and may change independently. The appearance layer is designed so that **appearance may fail, Harness keeps working**:

| State | Condition | What is applied |
| --- | --- | --- |
| `compatible` | Harness mounted and the `--dsw-*` token contract resolves | full theme + wallpaper + motion + controls |
| `degraded` | Harness mounted but the token contract is unavailable | wallpaper + the appearance button only (theming safely disabled) |
| `fallback` | Harness UI cannot be mounted/verified | nothing is injected; Harness is left untouched |

The current state is exposed as `data-hd-appearance-state` on `<html>` and in `window.__HD_STATE__` for diagnostics. An intentionally simulated mismatch (via `HD_COMPAT_MODE=simulate-degraded` / `simulate-fallback`) is used by the acceptance tests.

---

## Security boundary

- **No Harness Core modifications.** This project is separable from upstream; a Harness upgrade primarily requires compatibility re-validation, not a rebuild of the architecture.
- **No arbitrary theme code.** Themes are declarative JSON; there is no theme JavaScript execution, no shell, no unrestricted filesystem, no remote script execution, no theme marketplace/downloads.
- **Wallpaper is a narrow local boundary.** Selected images are validated, size-capped, copied to the app config dir, and served only to the main window via `hd-wallpaper://`. They are never uploaded, logged as image content, committed, or synchronized.
- **No credentials/sessions enter the repository or release artifacts.** Harness state stays in the user's `~/.dsh`, untouched by the appearance system.

---

## Repository layout

```
index.html / appearance.html   # loading page + settings window (Vite entries)
src/                           # TypeScript frontend
src-tauri/                     # Rust backend
  src/lib.rs                   # runtime resolution, lifecycle, wiring
  src/appearance.rs            # appearance state, themes, wallpaper, commands
  src/engine.js                # injected appearance engine (surface layer)
themes/                        # built-in + DIY theme sources
  ocean/                       # Ocean showcase
  starter/                     # documented starter template
```

---

## Attribution

- [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) — the upstream product this project wraps (MIT).
- The DeepSeek whale mark used decoratively in the Ocean theme is the original, unaltered silhouette from the official Harness favicon.

## Roadmap

V0.2 planned: standalone runtime with bundled Node + pinned Harness runtime.

## License

MIT — see [LICENSE](LICENSE). This license applies to Harness Desktop's own code; upstream DeepSeek Harness is governed by its own license.
