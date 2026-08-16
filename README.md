# DeepSeek Harness Desktop

> **非官方项目 · UNOFFICIAL PROJECT**
> DeepSeek Harness Desktop 是一个**非官方**社区项目，与 DeepSeek 无隶属、背书或关联关系。
> DeepSeek Harness Desktop is an **unofficial** community project, not affiliated with, endorsed by, or an official release of DeepSeek.

A native **macOS (Apple Silicon) + Windows (x64)** desktop shell for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness), plus a safe, user-customizable **appearance system**.

> **Keep Harness official. Make it yours.**

DeepSeek Harness Desktop launches a self-contained bundled runtime — pinned **Node v22.22.3** + pinned **`@deepseek-ai/dsh@0.1.0-rc.6`** — locally (`127.0.0.1`, dynamic port) and wraps the **official** DeepSeek Harness Web UI in a native window — without modifying Harness Core. On top of that, it adds a *surface-layer* appearance system: themes, a DIY starter theme, local image/MP4 backgrounds, and optional lightweight motion.

> **No Node setup. macOS + Windows. Clear Glass, local wallpapers, and a more reliable Harness desktop experience.**

> 中文说明见文末 → [中文说明](#中文说明)

---

## ⚠️ Unofficial project

DeepSeek Harness Desktop is an **independent, unofficial** project. It is not affiliated with, endorsed by, or associated with DeepSeek. The DeepSeek Harness software, its Web UI, and the DeepSeek whale mark remain the property of their respective owners.

- This project **reuses** the official Harness Web UI over its local HTTP server and does not fork or modify Harness Core. Since V0.2, DeepSeek Harness Desktop **distributes a pinned, unmodified `@deepseek-ai/dsh@0.1.0-rc.6` runtime plus a pinned Node binary** (see [Bundled runtime](#bundled-runtime)) — see `runtime/NOTICE.md` for the bundling statement and attribution.
- The Ocean showcase uses the official DeepSeek whale silhouette (unaltered) as a faint decorative element.
- MIT-licensed code in this repository is this project's own; upstream Harness (MIT) and Node.js (MIT-style) are redistributed under their own licenses with attribution preserved in `runtime/`.

---

## Project purpose

DeepSeek Harness already ships a capable Web UI (`dsh web`). DeepSeek Harness Desktop gives it a first-class native window and a clean way to *personalize the look* without ever touching Harness internals, credentials, sessions, or Agent behavior.

## Features

- **Native macOS + Windows window** around the official Harness Web UI (local-only serving, dynamic port, readiness handling, clean shutdown, child-process cleanup).
- **Self-contained runtime** — bundled pinned Node v22.22.3 + pinned `@deepseek-ai/dsh@0.1.0-rc.6`; no system Node/npm/npx/dsh and no runtime internet bootstrap. See [Bundled runtime](#bundled-runtime).
- **Appearance system** — surface-layer themes that sit *on top of* the official UI:
  - **Official** — the untouched Harness appearance.
  - **Ocean** — a polished, restrained ocean-inspired showcase.
  - **Starter (Sunset)** — a documented template for your own theme.
  - **Deep Glass** — clear scene + local glass surfaces (sidebar / composer / cards / dialogs), not full-screen frosted blur.
- **Glass Depth** — one simple control to tune surface transparency/depth.
- **Local media backgrounds** — PNG / JPG / JPEG / WebP images and local **MP4** video (muted, looped), served only through a local controlled boundary and never uploaded. Opacity / fit / position / blur / readability-overlay controls where supported.
- **DIY themes** — declarative JSON themes (tokens + constrained CSS + assets). No Harness Core changes, no forking, no arbitrary JavaScript.
- **Optional motion** — lightweight, decorative background motion for static backgrounds/themes; respects `prefers-reduced-motion`. MP4 does not receive extra cinematic pan/zoom (it supplies its own motion).
- **Safe fallback** — if an upstream Harness UI change makes customization unsafe, the appearance layer degrades or falls back without breaking Harness.
- **Desktop reliability** — an explicit health state machine (`STARTING → READY → HEALTHY`, with `DEGRADED`/`RECOVERING`/`FAILED`), owned-process-tree lifecycle, bounded health checks, and conservative recovery that never resubmits or duplicates a task.

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

**For end users (packaged V0.2):**

- **macOS Apple Silicon** (11+), or **Windows x64**
- **No system Node.js, npm, npx, or `dsh` install is required** — V0.2 ships a self-contained bundled runtime (pinned Node v22.22.3 + pinned `@deepseek-ai/dsh@0.1.0-rc.6`). See [Bundled runtime](#bundled-runtime).
- **Python is NOT required**

> V0.1 required a system Node/npm/dsh install and included an experimental automatic Harness bootstrap path. V0.2 removes that dependency for production use: the app uses its own bundled runtime and fails clearly (rather than silently falling back) if the bundled runtime is missing or corrupt.

**For developers/build contributors**, see [Installation / build](#installation--build).

---

## Bundled runtime

DeepSeek Harness Desktop V0.2 bundles a self-contained runtime so it does not depend on system `node`/`npm`/`npx`/`dsh`:

```
Harness Desktop → bundled Node v22.22.3 → pinned @deepseek-ai/dsh@0.1.0-rc.6 → dsh web → 127.0.0.1:<dynamic>
```

- Pinned Harness: `@deepseek-ai/dsh@0.1.0-rc.6` (never a silent upgrade/downgrade).
- Pinned Node: `v22.22.3` (matches the known-good V0.1 environment).
- **Integrity:** at startup, SHA-256 verifies the bundled Node executable and the Harness entrypoint before executing them. (The full `node_modules` dependency closure is pinned by the materializer's committed lockfile, but only the Node executable and Harness entrypoint are SHA-256 verified at launch.)
- Production startup performs **no** npm/npx/internet bootstrap and never silently falls back to arbitrary system Node/Harness; a missing/corrupt runtime fails closed with a clear error (integrity is verified **before** bundled code executes, and an integrity failure never falls back to the system runtime).
- The Harness remains bound to loopback (`127.0.0.1`, dynamic port); existing `~/.dsh` session semantics are untouched.
- Licensing/attribution: `runtime/NOTICE.md`, `runtime/THIRD_PARTY_LICENSES.json`, and the preserved `LICENSE` files inside `runtime/`. DeepSeek Harness and Node.js are redistributed under their own (MIT-style) licenses; third-party dependency licenses continue to apply (see the generated inventory).

The bundled runtime is a **build artifact**, reproduced from a clean checkout by a deterministic materializer (pinned Node artifact + official SHA-256 + `npm ci` from the committed lockfile):

```bash
npm run materialize:runtime
```

The committed inputs are `scripts/runtime-package.json` and `scripts/runtime-package-lock.json`; `runtime/` is gitignored (never commit the dependency closure). The materializer writes `runtime/manifest.json` (checksums), `runtime/NOTICE.md` + `runtime/THIRD_PARTY_LICENSES.json` (license inventory). The V0.2 release ships the `darwin-arm64` and `windows-x64` runtime targets.



---

## Installation / build

### For users

Download the packaged release for your platform (macOS Apple Silicon `.app`, or the Windows x64 NSIS installer), install/open it, and launch. Packaged V0.2 requires **no** separate Node/npm/npx/dsh setup.

### For developers / build contributors

Prerequisites:

- [Rust](https://rustup.rs) ≥ 1.77 (`rustup` + `cargo`)
- Node.js + npm **only for building/packaging** (used by the runtime materializer and Vite; not required to run the app)

```bash
npm install
npm run materialize:runtime   # materialize the bundled runtime (pinned Node + Harness)
npm run build:frontend        # builds the loading page + settings window (Vite)
cargo check                   # inside src-tauri/ — type-checks the Rust backend
npm run tauri build           # production app bundle (outputs to src-tauri/target/release/bundle)
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

## Usage

1. Launch **DeepSeek Harness Desktop**. The loading screen resolves the bundled runtime, pins it to Harness `0.1.0-rc.6`, starts `dsh web --host 127.0.0.1 --port 0`, waits for readiness, then opens the official Harness UI.
2. To change the look, either:
   - click the small **gear button** in the bottom-right corner of the Harness window, or
   - choose **DeepSeek Harness Desktop ▸ Appearance…** from the menu bar (`⌘,` on macOS).

The Appearance window lets you switch themes, pick an image/MP4 background and tune it, set Glass Depth, toggle motion, and reset to Official. Changes apply immediately and persist.

The Appearance window also has a **Language / 语言** selector with three options — **System / 跟随系统** (the default; a Chinese system locale shows Simplified Chinese, otherwise English), **简体中文**, and **English**. Your choice persists across restarts; choosing **System** keeps following the OS locale. This only localizes the Appearance settings window, never the official Harness UI.

> **Windows note (known V0.2 limitation).** On some Windows launches, the bundled Node child process may leave a visible console/CMD window open while Harness is running. This does not prevent Harness from working and is tracked as a polish item for V0.2.1.

> **Note on `@deepseek-ai/dsh` version pinning.** DeepSeek Harness Desktop refuses to start any Harness version other than `0.1.0-rc.6` (no silent upgrade or downgrade). This keeps the desktop shell honest about what it was validated against. See [Compatibility & fallback](#compatibility--fallback).

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

## Wallpaper / local media

- Click **Choose image or video…** and select a **PNG**, **JPG/JPEG**, **WebP** image, or a local **MP4** video.
- Images tune **fit** (cover/contain/fill/original), **position**, **opacity**, **blur**, and the **readability overlay**.
- MP4 videos are **muted + looped**, served only through a local controlled boundary, and support the same fit/position/opacity controls where applicable (video blur is only applied conservatively via CSS/filter).
- The overlay preserves text contrast over light or busy media; choose **Auto** (follows the active Harness theme), **Dark**, or **Light**.
- **Remove wallpaper** clears it. Media persists across restarts and is stored only in the app config directory — never uploaded.

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
- **Media is a narrow local boundary.** Selected images/videos are validated, size-capped, copied to the app config dir, and served only to the main window via the local `hd-wallpaper://` scheme. They are never uploaded, logged as content, committed, or synchronized.
- **No credentials/sessions enter the repository or release artifacts.** Harness state stays in the user's `~/.dsh`, untouched by the appearance system.
- **The Harness backend remains loopback-only** (`127.0.0.1`, dynamic port).
- **Remote access is NOT part of V0.2.0.** No public listener, no remote gateway, no pairing/transport is shipped in this release.

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

- V0.2 (current): self-contained bundled runtime, macOS Apple Silicon + Windows x64, desktop reliability, Deep Glass appearance, local image + MP4 backgrounds, integrity + compatibility/fallback boundaries.
- V0.2.1 (planned / experimental): remote access from phone/browser (authenticated Desktop-controlled gateway, secure transport/pairing), hiding the Windows Node console window, and a revisit of the inline usage/cost UX.

## License

MIT — see [LICENSE](LICENSE). This license applies to DeepSeek Harness Desktop's own code; upstream DeepSeek Harness is governed by its own license.

---

## 中文说明

DeepSeek Harness Desktop 是 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的原生 **macOS（Apple Silicon）+ Windows（x64）** 桌面外壳，并附带一套安全、可自定义的 **外观系统**。

> **保持 Harness 原貌，把它变成你的。**

DeepSeek Harness Desktop 会在本地启动一个**自包含的内置运行时**——锁定 **Node v22.22.3** + 锁定 **`@deepseek-ai/dsh@0.1.0-rc.6`**（`127.0.0.1`，动态端口），并在原生窗口中包裹**官方** DeepSeek Harness Web UI——不修改 Harness 核心。在此基础上，它叠加了一层*表层*外观系统：主题、DIY 入门主题、本地图片/MP4 背景，以及可选的轻量动效。

> **免装 Node。macOS + Windows。Clear Glass 清透玻璃、本地壁纸，以及更可靠的 Harness 桌面体验。**

### ⚠️ 非官方项目

DeepSeek Harness Desktop 是一个**独立的、非官方**项目。它与 DeepSeek 无隶属、背书或关联关系。DeepSeek Harness 软件、其 Web UI 以及 DeepSeek 鲸鱼标志均归其各自所有者所有。

- 本项目**复用**官方 Harness Web UI（经由其本地 HTTP 服务），并不 fork、也不修改 Harness 核心。自 V0.2 起，DeepSeek Harness Desktop **分发锁定版本的、未经修改的 `@deepseek-ai/dsh@0.1.0-rc.6` 运行时以及锁定版本的 Node 二进制**（见[内置运行时](#内置运行时)）——打包声明与归属见 `runtime/NOTICE.md`。
- Ocean 主题将官方 DeepSeek 鲸鱼剪影（未改动）作为淡色装饰元素使用。
- 本仓库中的 MIT 许可代码属于本项目自身；上游 Harness（MIT）与 Node.js（MIT 风格）按其自身许可证再分发，归属信息保留在 `runtime/` 中。

### 项目目的

DeepSeek Harness 已自带一个功能完善的 Web UI（`dsh web`）。DeepSeek Harness Desktop 为它提供了一流的原生窗口，以及一种在不触碰 Harness 内部、凭据、会话或 Agent 行为的前提下*个性化外观*的干净方式。

### 功能特性

- **原生 macOS + Windows 窗口**——包裹官方 Harness Web UI（仅本地服务、动态端口、就绪处理、干净退出、子进程清理）。
- **自包含运行时**——内置锁定 Node v22.22.3 + 锁定 `@deepseek-ai/dsh@0.1.0-rc.6`；无需系统 Node/npm/npx/dsh，也无需运行时联网引导。见[内置运行时](#内置运行时)。
- **外观系统**——叠加在官方 UI *之上*的表层主题：
  - **Official（官方）**——未改动的 Harness 外观。
  - **Ocean（海洋）**——一套精致、克制的海洋风格展示。
  - **Starter（日落入门）**——一份带文档的主题模板，供你制作自己的主题。
  - **Deep Glass（深玻璃）**——清晰场景 + 局部玻璃表面（侧栏 / 输入框 / 卡片 / 对话框），而非全屏磨砂模糊。
- **玻璃深度**——一个简单控件即可调节表面透明度/深度。
- **本地媒体背景**——PNG / JPG / JPEG / WebP 图片与本地 **MP4** 视频（静音、循环），仅通过本地受控边界提供，绝不上传。支持透明度 / 填充 / 位置等控件。
- **DIY 主题**——声明式 JSON 主题（token + 受限 CSS + 资源）。不改 Harness 核心、不 fork、不执行任意 JavaScript。
- **可选动效**——静态背景/主题的轻量装饰性背景动效，尊重 `prefers-reduced-motion`；MP4 不叠加额外的电影感缩放/平移（视频自带运动）。
- **安全回退**——若上游 Harness UI 变更使自定义不再安全，外观层会降级或回退，而不会破坏 Harness。
- **桌面可靠性**——显式健康状态机（`STARTING → READY → HEALTHY`，含 `DEGRADED`/`RECOVERING`/`FAILED`）、自有进程树生命周期、有界健康检查，以及绝不重发或重复任务的保守恢复。

### 架构概览

```
┌────────────────────────────────────────────────────────────┐
│ Tauri (Rust)                                                │
│  • 解析并在 127.0.0.1:<动态端口> 启动 `dsh web`               │
│  • 管理子进程组与关闭                                        │
│  • 持有外观状态（持久化到应用配置目录）                        │
│  • 通过 `hd-wallpaper://` 提供本地壁纸                        │
│  • 向设置窗口暴露窄范围的 IPC 命令                            │
└───────────────┬──────────────────────────────┬─────────────┘
                │ 导航                           │ IPC（自有源）
        ┌───────▼────────┐              ┌───────▼──────────────┐
        │ 主窗口           │              │ 设置窗口              │
        │ (Harness UI)    │              │ (appearance.html)    │
        │ + 注入的         │              │ 主题/壁纸/动效         │
        │   engine.js     │              │ 控制项                │
        └─────────────────┘              └──────────────────────┘
```

### 关键设计决策

1. **永不修改 Harness Web UI。** WebView 以 `dsh web` 提供的内容原样导航到官方 UI。外观由自包含的 `engine.js`（一个*初始化脚本*）注入，它只向 `<body>` 添加一个 `<style>` 元素和少量 `pointer-events:none` 背景层。它从不触碰 `#root` 内部的 React 树。
2. **主题是声明式的，不是代码。** 主题是一个 JSON 对象，包含 token 覆盖（官方 `--dsw-*` CSS 自定义属性契约）、可选的作用域组件 CSS、可选的动效 CSS，以及可选的氛围资源。**没有**主题 JavaScript、没有 shell 访问、也没有文件系统/凭据访问。
3. **壁纸始终留在本地。** 选中的图片会被校验并复制到应用配置目录（`~/Library/Application Support/…/wallpaper/`），绝不会进入仓库、绝不上传、也绝不把图片内容记入日志。
4. **兼容性是一等状态。** 加载时引擎会校验 (a) `#root` 已挂载，以及 (b) 核心主题 token 契约仍可解析。然后它应用三种状态之一——`compatible`（兼容）、`degraded`（降级，仅壁纸）或 `fallback`（回退，什么都不注入）——这样上游变更就不会把外观不匹配演变成应用崩溃。

### 系统要求

**面向最终用户（V0.2 打包版）：**

- **macOS Apple Silicon**（11+），或 **Windows x64**
- **无需安装系统 Node.js、npm、npx 或 `dsh`** —— V0.2 自带自包含内置运行时（锁定 Node v22.22.3 + 锁定 `@deepseek-ai/dsh@0.1.0-rc.6`）。见[内置运行时](#内置运行时)。
- **不需要 Python**

> V0.1 需要系统 Node/npm/dsh 安装，并包含一条实验性的自动 Harness 引导路径。V0.2 在生产使用中去掉了这一依赖：应用使用自己的内置运行时，若内置运行时缺失或损坏会清晰报错（而非静默回退）。

**面向开发者/构建贡献者**，见[安装 / 构建](#安装--构建)。

---

### 内置运行时

DeepSeek Harness Desktop V0.2 内置了一个自包含运行时，因此不依赖系统 `node`/`npm`/`npx`/`dsh`：

```
Harness Desktop → 内置 Node v22.22.3 → 锁定 @deepseek-ai/dsh@0.1.0-rc.6 → dsh web → 127.0.0.1:<动态端口>
```

- 锁定 Harness：`@deepseek-ai/dsh@0.1.0-rc.6`（绝不静默升级/降级）。
- 锁定 Node：`v22.22.3`（与 V0.1 已验证环境一致；完整性见 `runtime/manifest.json` 校验和，启动时校验）。
- 生产启动**不做** npm/npx/联网引导，也绝不静默回退到任意的系统 Node/Harness；内置运行时缺失或损坏会清晰报错并关闭（完整性在执行内置代码**之前**校验，完整性失败绝不回退到系统运行时）。
- Harness 仍仅绑定回环地址（`127.0.0.1`，动态端口）；现有 `~/.dsh` 会话语义保持不变。
- 许可与归属：`runtime/NOTICE.md`、`runtime/THIRD_PARTY_LICENSES.json` 以及 `runtime/` 内保留的 `LICENSE` 文件。DeepSeek Harness 与 Node.js 按其自身（MIT 风格）许可证再分发；第三方依赖许可证继续适用（见生成的清单）。

内置运行时是**构建产物**，由确定性物化脚本从干净检出复现（锁定 Node 制品 + 官方 SHA-256 + 依据提交的 lockfile 执行 `npm ci`）：

```bash
npm run materialize:runtime
```

提交的输入为 `scripts/runtime-package.json` 与 `scripts/runtime-package-lock.json`；`runtime/` 被 gitignore（绝不提交依赖闭包）。物化脚本会写入 `runtime/manifest.json`（校验和）、`runtime/NOTICE.md` + `runtime/THIRD_PARTY_LICENSES.json`（许可清单）。V0.2 发布版随附 `darwin-arm64` 与 `windows-x64` 两个运行时目标。



### 安装 / 构建

前置条件（macOS 11+）：

- [Rust](https://rustup.rs) ≥ 1.77（`rustup` + `cargo`）
- 一个已物化的 `runtime/` 目录（内置 Node + Harness）——见[内置运行时](#内置运行时)

```bash
npm install
npm run build:frontend   # 构建加载页 + 设置窗口（Vite）
cargo check              # 在 src-tauri/ 内——类型检查 Rust 后端
npm run tauri build      # 生产 .app 包（输出到 src-tauri/target/release/bundle）
```

开发循环：

```bash
npm run tauri dev        # 运行 Vite + cargo run，启动应用
```

#### 回归检查

```bash
npm run typecheck                        # tsc --noEmit
npm run build:frontend                   # Vite 生产构建
(cd src-tauri && cargo test)             # Rust 单元测试
(cd src-tauri && cargo clippy -- -D warnings)   # 静态检查（推荐）
npm run tauri build                      # 生产 Tauri 构建
```

### 使用

1. 启动 **DeepSeek Harness Desktop**。加载页会解析内置运行时，将其锁定到 Harness `0.1.0-rc.6`，启动 `dsh web --host 127.0.0.1 --port 0`，等待就绪，然后打开官方 Harness UI。
2. 要更改外观，可以：
   - 点击 Harness 窗口右下角的小**齿轮按钮**，或
   - 从菜单栏选择 **DeepSeek Harness Desktop ▸ Appearance…**（macOS 上为 `⌘,`）。

外观窗口可让你切换主题、选择并调节图片/MP4 背景、设置玻璃深度、开关动效，并重置为 Official。更改即时生效并持久保存。

外观窗口还有一个 **Language / 语言** 选择器，共三个选项——**System / 跟随系统**（默认；中文系统语言环境显示简体中文，否则显示英文）、**简体中文** 和 **English**。你的选择会跨重启保留；选择 **System** 则持续跟随系统语言环境。这只本地化外观设置窗口，绝不本地化官方 Harness UI。

> **Windows 说明（V0.2 已知限制）。** 在部分 Windows 启动场景下，内置 Node 子进程可能会在 Harness 运行期间保持一个可见的控制台/CMD 窗口。这不会妨碍 Harness 正常工作，已作为 V0.2.1 的打磨项跟踪。

> **关于 `@deepseek-ai/dsh` 版本锁定的说明。** DeepSeek Harness Desktop 拒绝启动 `0.1.0-rc.6` 以外的任何 Harness 版本（不会静默升级或降级）。这让桌面外壳如实说明它针对哪个版本做了验证。见[兼容性与回退](#compatibility--fallback)。

### DIY 主题

一个主题就是一个包含 `theme.json` 和（可选）`assets/` 目录的文件夹。复制 [`themes/starter`](themes/starter) 文件夹，修改几个值，然后放入：

```
~/Library/Application Support/com.deepseek.harnessdesktop/themes/<你的主题-id>/theme.json
```

主题可以更改：

| 方面 | 位置 | 示例 |
| --- | --- | --- |
| 颜色 / token | `tokens.light` / `tokens.dark` | `"--dsw-alias-brand-primary": "rgb(25,108,150)"` |
| 组件样式 | `components`（作用域 CSS） | 圆角输入框、调整选中高亮 |
| 装饰资源 | `asset` | 渐变或 `assets/whale.svg` |
| 动效 | `motion`（CSS 关键帧） | 缓慢的环境漂移 |
| 壁纸表面 | `surfaces`（半透明 token） | 壁纸激活时的玻璃效果 |

完整的分步教程见 **[themes/starter/README.md](themes/starter/README.md)**（具备前端能力的开发者大约 10–20 分钟即可做出第一个主题）。

**安全性：** 主题是不可信的外观内容。引擎只接受自定义属性 token 覆盖，并把 `components`/`motion` 作为 *CSS 文本*注入（绝不是 JavaScript）。主题无法触及 Harness 凭据、会话、shell 或 Agent 权限。

#### 主题 CSS 与资源安全

`components` 和 `motion` 在到达 WebView 之前会经过真正的 CSS 解析器校验，任何违规都会拒绝整个主题（fail-closed）：

- **选择器作用域**——每个选择器必须以 `#root` 开头，且只能向下（`>` 子级或空格后代）。`html`、`body`、`:root`、兄弟/列组合器、`:has()` 以及任何可能逃逸出 `#root` 的内容都会被拒绝。`:is()`、`:where()`、`:not()` 和 `:nth-child(... of …)` 会被递归校验。
- **At 规则**——只允许 `@media` 和 `@supports`（内部规则递归校验）。`@keyframes` 仅允许 `hd-` 名称前缀；其选择器为 `from` / `to` / 百分比。`@import`、`@font-face` 和未知 at 规则会被拒绝。
- **禁止携带 URL 的 CSS**——`url(...)`（远程字体、背景 URL、任何协议）会被拒绝。

装饰性 `asset` 值被限制在主题自身的 `assets/` 目录内：

- 允许类型：**SVG / PNG / JPG / JPEG / WebP**，每个 ≤ **512 KB**，对 PNG/JPEG/WebP 进行魔数校验。
- `assets/<file>` 引用通过规范路径包含关系解析——`../`、绝对路径、反斜杠、符号链接以及跨主题访问都会被拒绝。不存在的文件会被拒绝。
- 此外，`asset` 也可以是 CSS 渐变或 `data:image/*;base64,…` URI（仅图片 MIME 类型，大小受限）；`file:`、`http:`、`https:` 和协议相对 URL 会被拒绝。

### 壁纸 / 本地媒体

- 点击 **Choose image or video…（选择图片或视频）** 并选择一个 **PNG**、**JPG/JPEG**、**WebP** 图片，或一个本地 **MP4** 视频。
- 图片可调节 **fit（填充方式）**（cover/contain/fill/original）、**position（位置）**、**opacity（透明度）**、**blur（模糊）** 和 **readability overlay（可读性遮罩）**。
- MP4 视频为**静音 + 循环**，仅通过本地受控边界提供，并在适用处支持同样的填充/位置/透明度控件（视频模糊仅在性能允许时谨慎应用）。
- 遮罩在明亮或复杂的媒体上保持文字对比度；可选 **Auto（自动，跟随当前 Harness 主题）**、**Dark（深色）** 或 **Light（浅色）**。
- **Remove wallpaper（移除壁纸）** 会清除它。媒体跨重启保留，且只保存在应用配置目录中——绝不上传。

不支持的、被改名的、过大的或损坏的文件会在写入任何内容之前被拒绝。

### 动效与减少动态效果

动效是**可选的**（在外观窗口中开关），且仅用于装饰：

- 动画仅使用 CSS、对 GPU 友好，绝不阻塞点击、文本输入、滚动或导航；
- 系统的**减少动态效果**设置（`prefers-reduced-motion`）会自动禁用主题动效，且始终被尊重；
- 任何装饰都不会拦截指针交互（所有注入层都是 `pointer-events: none`）。

### 兼容性与回退

DeepSeek Harness 是上游软件，可能独立变化。外观层的设计原则是**外观可以失败，但 Harness 继续工作**：

| 状态 | 条件 | 应用内容 |
| --- | --- | --- |
| `compatible` | Harness 已挂载且 `--dsw-*` token 契约可解析 | 完整主题 + 壁纸 + 动效 + 控件 |
| `degraded` | Harness 已挂载但 token 契约不可用 | 仅壁纸 + 外观按钮（主题被安全禁用） |
| `fallback` | Harness UI 无法挂载/校验 | 什么都不注入；Harness 保持原样 |

当前状态通过 `<html>` 上的 `data-hd-appearance-state` 和 `window.__HD_STATE__` 暴露，用于诊断。验收测试使用刻意模拟的不匹配（`HD_COMPAT_MODE=simulate-degraded` / `simulate-fallback`）。

### 安全边界

- **不修改 Harness 核心。** 本项目与上游可分离；Harness 升级主要需要重新做兼容性验证，而不是重建架构。
- **不执行任意主题代码。** 主题是声明式 JSON；不存在主题 JavaScript 执行、没有 shell、没有不受限的文件系统、没有远程脚本执行、没有主题市场/下载。
- **壁纸是窄范围的本地边界。** 选中的图片会被校验、限制大小、复制到应用配置目录，并仅通过 `hd-wallpaper://` 提供给主窗口。它们绝不被上传、不被记录图片内容、不被提交、也不被同步。
- **凭据/会话不会进入仓库或发布产物。** Harness 状态保存在用户的 `~/.dsh` 中，外观系统不会触碰。

### 仓库结构

```
index.html / appearance.html   # 加载页 + 设置窗口（Vite 入口）
src/                           # TypeScript 前端
src-tauri/                     # Rust 后端
  src/lib.rs                   # 运行时解析、生命周期、接线
  src/appearance.rs            # 外观状态、主题、壁纸、命令
  src/engine.js                # 注入的外观引擎（表层）
themes/                        # 内置 + DIY 主题源
  ocean/                       # Ocean 展示主题
  starter/                     # 带文档的入门模板
```

### 归属声明

- [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)——本项目所包裹的上游产品（MIT）。
- Ocean 主题中作为装饰使用的 DeepSeek 鲸鱼标志，是来自官方 Harness favicon 的原始、未改动剪影。

### 路线图

- V0.2（当前）：自包含内置运行时、macOS Apple Silicon + Windows x64、桌面可靠性、Deep Glass 外观、本地图片 + MP4 背景、完整性与兼容/回退边界。
- V0.2.1（规划 / 实验）：手机/浏览器远程访问（经身份验证的桌面控制网关、安全传输/配对）、隐藏 Windows Node 控制台窗口，以及重新审视内联用量/成本体验。

### 许可证

MIT——见 [LICENSE](LICENSE)。该许可证适用于 DeepSeek Harness Desktop 自身的代码；上游 DeepSeek Harness 按其自身许可证管理。
