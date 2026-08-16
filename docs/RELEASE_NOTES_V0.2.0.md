# DeepSeek Harness Desktop v0.2.0

DeepSeek Harness Desktop is an **unofficial** desktop wrapper around the official
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) Web UI.

Its design principle is:

> **Wrap, don't fork.**

V0.2 turns the project from a macOS wrapper that required external runtime setup
into a self-contained, cross-platform desktop package. Harness Core is never
modified; the app wraps the official UI and adds a safe, customizable appearance
layer on top.

---

## Highlights

### No Node setup
V0.2 bundles pinned **Node v22.22.3** and pinned **`@deepseek-ai/dsh@0.1.0-rc.6`**.
End users do not separately install Node, npm, npx, or dsh. At startup, SHA-256
verifies the bundled Node executable and Harness entrypoint before executing them.

### Windows x64
A Windows x64 build with an NSIS installer and real Windows CI.

### Reliable desktop lifecycle
Loopback-only backend, dynamic port, readiness handling, an explicit
health/reliability state, owned child-process cleanup, and conservative recovery
that never resubmits or duplicates a task.

### Deep Glass
A **Clear Scene + Local Glass** appearance — the wallpaper/video stays optically
clear in open areas, while the sidebar, composer, cards, and dialogs are local
glass surfaces. Not full-screen frosted blur.

### Local media backgrounds
Local **PNG / JPG / JPEG / WebP** images and local **MP4** video (muted, looped),
stored locally and served through a controlled local boundary — never uploaded.

### Compatibility first
Appearance is a surface layer. If upstream Harness UI targeting ever breaks, the
appearance layer degrades or falls back without breaking Harness.

---

## Tested

- **macOS Apple Silicon** — Human E2E: **PASS** (Deep Glass, image + MP4 backgrounds, Glass Depth, foreground readability).
- **macOS build/tests** — **PASS** (typecheck, frontend build, engine/guard/engine-apply/harness-compat tests, `cargo test`, `cargo clippy`, production build).
- **Windows x64** — Installer: **PASS**; Launch-to-Harness: **PASS**; bundled runtime: **PASS**.
- **Windows CI** — integrated release-candidate workflow result is the mandatory release gate (see below).
- **Windows final session/restart** — not independently re-tested in the final human gate.

---

## Known limitations

- This is an **unofficial** project — not affiliated with or endorsed by DeepSeek.
- Pinned to `@deepseek-ai/dsh@0.1.0-rc.6` (no silent upgrade/downgrade).
- Release binaries are **unsigned / not notarized** (macOS may show a Gatekeeper prompt; Windows may show a SmartScreen prompt).
- **Windows x64 only** for this release (no Windows ARM, no macOS Intel).
- On some Windows launches, the bundled Node child process may leave a visible console/CMD window open while Harness is running (known, non-blocking; tracked for V0.2.1).
- **Remote access is not included** in V0.2.0.
- Upstream Harness UI changes may require appearance-compatibility revalidation.

---

## Next

V0.2.1 (planned / experimental): remote access from phone/browser (authenticated
Desktop-controlled gateway, secure transport/pairing), hiding the Windows Node
console window, and a revisit of the inline usage/cost UX.
