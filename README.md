# DeepSeek Harness Desktop

> **非官方项目 · UNOFFICIAL PROJECT**
> DeepSeek Harness Desktop 是一个**非官方**社区项目，与 DeepSeek 无隶属、背书或关联关系。
> DeepSeek Harness Desktop is an **unofficial** community project, not affiliated with, endorsed by, or an official release of DeepSeek.

A native **macOS** desktop shell for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness), plus a safe, user-customizable **appearance system**.

> **Keep Harness official. Make it yours.**

DeepSeek Harness Desktop launches your existing `@deepseek-ai/dsh` runtime locally (`127.0.0.1`, dynamic port) and wraps the **official** DeepSeek Harness Web UI in a native window — without modifying Harness Core. On top of that, it adds a *surface-layer* appearance system: user-selectable themes, a DIY starter theme, a local wallpaper, and optional lightweight motion.

> 中文说明见文末 → [中文说明](#中文说明)

---

## ⚠️ Unofficial project

DeepSeek Harness Desktop is an **independent, unofficial** project. It is not affiliated with, endorsed by, or associated with DeepSeek. The DeepSeek Harness software, its Web UI, and the DeepSeek whale mark remain the property of their respective owners.

- This project **reuses** the official Harness Web UI over its local HTTP server; it does not bundle, fork, or modify Harness Core.
- The Ocean showcase uses the official DeepSeek whale silhouette (unaltered) as a faint decorative element.
- MIT-licensed code in this repository is this project's own; upstream Harness is distributed under its own license.

---

## Project purpose

DeepSeek Harness already ships a capable Web UI (`dsh web`). DeepSeek Harness Desktop gives it a first-class native window and a clean way to *personalize the look* without ever touching Harness internals, credentials, sessions, or Agent behavior.

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

> V0.1 contains an experimental automatic Harness bootstrap path, but a clean first-install bootstrap is **NOT guaranteed**. Install DeepSeek Harness `0.1.0-rc.6` yourself before relying on DeepSeek Harness Desktop.

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

1. Launch **DeepSeek Harness Desktop**. The loading screen resolves your runtime, pins it to Harness `0.1.0-rc.6`, starts `dsh web --host 127.0.0.1 --port 0`, waits for readiness, then opens the official Harness UI.
2. To change the look, either:
   - click the small **gear button** in the bottom-right corner of the Harness window, or
   - choose **DeepSeek Harness Desktop ▸ Appearance…** from the menu bar (`⌘,`).

The Appearance window lets you switch themes, pick a wallpaper and tune it, toggle motion, and reset to Official. Changes apply immediately and persist.

The Appearance window also has a **Language / 语言** selector with three options — **System / 跟随系统** (the default; a Chinese system locale shows Simplified Chinese, otherwise English), **简体中文**, and **English**. Your choice persists across restarts; choosing **System** keeps following the OS locale. This only localizes the Appearance settings window, never the official Harness UI.

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

MIT — see [LICENSE](LICENSE). This license applies to DeepSeek Harness Desktop's own code; upstream DeepSeek Harness is governed by its own license.

---

## 中文说明

DeepSeek Harness Desktop 是 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的原生 **macOS** 桌面外壳，并附带一套安全、可自定义的 **外观系统**。

> **保持 Harness 原貌，把它变成你的。**

DeepSeek Harness Desktop 会在本地启动你现有的 `@deepseek-ai/dsh` 运行时（`127.0.0.1`，动态端口），并在原生窗口中包裹**官方** DeepSeek Harness Web UI——不修改 Harness 核心。在此基础上，它叠加了一层*表层*外观系统：可选主题、DIY 入门主题、本地壁纸，以及可选的轻量动效。

### ⚠️ 非官方项目

DeepSeek Harness Desktop 是一个**独立的、非官方**项目。它与 DeepSeek 无隶属、背书或关联关系。DeepSeek Harness 软件、其 Web UI 以及 DeepSeek 鲸鱼标志均归其各自所有者所有。

- 本项目**复用**官方 Harness Web UI（经由其本地 HTTP 服务），并不打包、不 fork、也不修改 Harness 核心。
- Ocean 主题将官方 DeepSeek 鲸鱼剪影（未改动）作为淡色装饰元素使用。
- 本仓库中的 MIT 许可代码属于本项目自身；上游 Harness 按其自身许可分发。

### 项目目的

DeepSeek Harness 已自带一个功能完善的 Web UI（`dsh web`）。DeepSeek Harness Desktop 为它提供了一流的原生窗口，以及一种在不触碰 Harness 内部、凭据、会话或 Agent 行为的前提下*个性化外观*的干净方式。

### 功能特性

- **原生 macOS 窗口**——包裹官方 Harness Web UI（仅本地服务、动态端口、就绪处理、干净退出、子进程清理）。
- **外观系统**——叠加在官方 UI *之上*的表层主题：
  - **Official（官方）**——未改动的 Harness 外观。
  - **Ocean（海洋）**——一套精致、克制的海洋风格展示。
  - **Starter（日落入门）**——一份带文档的主题模板，供你制作自己的主题。
- **DIY 主题**——声明式 JSON 主题（token + 受限 CSS + 资源）。不改 Harness 核心、不 fork、不执行任意 JavaScript。
- **本地壁纸**——PNG / JPG / JPEG / WebP，支持填充方式、位置、透明度、模糊与可读性遮罩。重启后保留；仅保存在本地，绝不上传。
- **可选动效**——轻量、装饰性，尊重 `prefers-reduced-motion`。
- **安全回退**——若上游 Harness UI 变更使自定义不再安全，外观层会降级或回退，而不会破坏 Harness。

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

- **macOS**（11+）
- **Node.js**（≥ 20）与 npm
- **DeepSeek Harness `0.1.0-rc.6`**（`@deepseek-ai/dsh`）
- **不需要 Python**

> V0.1 包含一条实验性的自动 Harness 引导路径，但干净的首次安装引导**不保证成功**。在依赖 DeepSeek Harness Desktop 之前，请自行安装 DeepSeek Harness `0.1.0-rc.6`。

### 安装 / 构建

前置条件（macOS 11+）：

- [Node.js](https://nodejs.org) ≥ 20 与 npm
- [Rust](https://rustup.rs) ≥ 1.77（`rustup` + `cargo`）
- 一个可用的 DeepSeek Harness 安装（`@deepseek-ai/dsh@0.1.0-rc.6`）——见[使用](#macos-usage)

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

### macOS 使用

1. 启动 **DeepSeek Harness Desktop**。加载页会解析你的运行时，将其锁定到 Harness `0.1.0-rc.6`，启动 `dsh web --host 127.0.0.1 --port 0`，等待就绪，然后打开官方 Harness UI。
2. 要更改外观，可以：
   - 点击 Harness 窗口右下角的小**齿轮按钮**，或
   - 从菜单栏选择 **DeepSeek Harness Desktop ▸ Appearance…**（`⌘,`）。

外观窗口可让你切换主题、选择并调节壁纸、开关动效，并重置为 Official。更改即时生效并持久保存。

外观窗口还有一个 **Language / 语言** 选择器，共三个选项——**System / 跟随系统**（默认；中文系统语言环境显示简体中文，否则显示英文）、**简体中文** 和 **English**。你的选择会跨重启保留；选择 **System** 则持续跟随系统语言环境。这只本地化外观设置窗口，绝不本地化官方 Harness UI。

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

### 壁纸

- 点击 **Choose image…（选择图片）** 并选择一个 **PNG**、**JPG/JPEG** 或 **WebP** 文件。
- 调节 **fit（填充方式）**（cover/contain/fill/original）、**position（位置）**、**opacity（透明度）**、**blur（模糊）** 和 **readability overlay（可读性遮罩）**。
- 遮罩在明亮或复杂的图片上保持文字对比度；可选 **Auto（自动，跟随当前 Harness 主题）**、**Dark（深色）** 或 **Light（浅色）**。
- **Remove wallpaper（移除壁纸）** 会清除它。壁纸跨重启保留，且只保存在应用配置目录中。

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

V0.2 计划：独立运行时，附带内置 Node 与锁定的 Harness 运行时。

### 许可证

MIT——见 [LICENSE](LICENSE)。该许可证适用于 DeepSeek Harness Desktop 自身的代码；上游 DeepSeek Harness 按其自身许可证管理。
