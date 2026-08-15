# Starter theme — make your own appearance in ~10–20 minutes

This folder is a **copyable template**. You never edit Harness Core; you only
edit this JSON. DeepSeek Harness Desktop turns it into a `<style>` overlay on top of the
official DeepSeek Harness UI.

## 1. Copy the folder

Copy this whole folder somewhere convenient and rename the folder to your theme id:

```
cp -R themes/starter ~/Library/Application\ Support/com.deepseek.harnessdesktop/themes/sunset
```

The folder name (`sunset`) becomes the theme id. Relaunch DeepSeek Harness Desktop, open
**Appearance**, and your theme appears in the list.

> Built-in themes (`official`, `ocean`, `starter`) live inside the app; anything
> else under `themes/` is a custom theme.

## 2. Understand `theme.json`

```jsonc
{
  "id": "starter",            // unique id (use the folder name)
  "name": "Starter (Sunset)", // shown in the Appearance window
  "description": "…",         // short description shown in the UI

  // ── Colors / tokens ────────────────────────────────────────────────
  // The official Harness UI is fully tokenized with --dsw-* CSS variables.
  // Override any of them here. "light" and "dark" are applied automatically
  // based on the active Harness theme (system/light/dark).
  "tokens": {
    "light": { "--dsw-alias-bg-base": "rgb(252,249,245)", "…": "…" },
    "dark":  { "--dsw-alias-bg-base": "rgb(26,21,20)",  "…": "…" }
  },

  // ── Wallpaper surfaces (optional) ──────────────────────────────────
  // When a wallpaper is active, these translucent surface colors are used so
  // the wallpaper shows through as a glass effect. Omit to use safe defaults.
  "surfaces": {
    "light": { "--dsw-alias-bg-base": "rgba(252,249,245,0.84)", "…": "…" },
    "dark":  { "--dsw-alias-bg-base": "rgba(26,21,20,0.72)",   "…": "…" }
  },

  // ── Component styling (optional) ───────────────────────────────────
  // Plain CSS, scoped and validated by the loader. Every selector MUST begin
  // with `#root` and may only descend (child `>` / descendant space). This is
  // enforced by a real CSS parser — invalid CSS rejects the whole theme.
  // Prefer STABLE selectors: #root, #root [contenteditable], #root ::selection,
  // #root input/textarea/button, #root ::-webkit-scrollbar.
  // Hashed class names (e.g. ._frame_9gj4p_6) change between upstream releases
  // and are treated as best-effort.
  "components": "#root [contenteditable=\"true\"] { border-radius: 14px; }",

  // ── Motion (optional) ──────────────────────────────────────────────
  // CSS keyframes + rules. Disabled automatically under "Reduce Motion" and
  // the Appearance ▸ Motion toggle. Keep it light and decorative.
  // @keyframes names must use the `hd-` prefix; selectors must begin with #root.
  "motion": "@keyframes hd-starter-drift { to { background-position: 0 80px; } } #root > * { animation: hd-starter-drift 26s linear infinite; }",

  // ── Decorative asset (optional) ────────────────────────────────────
  // One of: a CSS gradient, a `data:image/*;base64,…` URI, or `assets/foo.svg`
  // (a file in this theme's own assets/ folder — SVG/PNG/JPG/WebP, ≤ 512 KB,
  // inlined by the loader). The file path cannot use `..`, absolute paths,
  // backslashes, symlinks, or any remote URL.
  "asset": "linear-gradient(180deg, #3a1d2e 0%, #1c1116 100%)"
}
```

## 3. Change the visible things (the fast path)

Try these five edits — you should see each one immediately after switching back
to your theme:

1. **Colors/tokens** — change `tokens.light["--dsw-alias-bg-base"]` and
   `tokens.dark["--dsw-alias-bg-base"]` to any `rgb()` color.
2. **Accent color** — change `--dsw-alias-brand-primary` (light) and the same in
   dark; this tints primary buttons, links, and the active accent.
3. **Component treatment** — change the `border-radius` in `components` to `22px`.
4. **Decorative asset** — replace `asset` with a `linear-gradient(...)` of your
   own, or add `assets/hero.svg` and set `"asset": "assets/hero.svg"`.
5. **Motion** — change the keyframe duration from `26s` to `8s`.

## 4. Useful token cheat-sheet

These are the semantic tokens the whole UI reads (light + dark each):

| Token | What it controls |
| --- | --- |
| `--dsw-alias-bg-base` | app background |
| `--dsw-alias-bg-layer-1/2/3` | elevated surfaces |
| `--dsw-specific-sidebar-fill` | the sidebar column |
| `--dsw-alias-label-primary` | primary text |
| `--dsw-alias-label-secondary/tertiary` | secondary/dim text |
| `--dsw-alias-brand-primary` | primary brand color |
| `--dsw-alias-state-business-primary` | links/info/active accents |
| `--dsw-alias-border-l1/l2` | borders |
| `--dsw-alias-interactive-bg-hover` | hover backgrounds |

There are many more (`--dsw-alias-button-*`, `--dsw-alias-markdown-*`,
`--dsw-alias-state-*`, …). The full set lives in Harness's `design-platform.css`;
you only need to override the ones you care about.

## 5. Rules & safety

- **Only `--`-prefixed custom-property values are accepted in `tokens`.** The
  engine rejects anything that looks like a full CSS declaration, so a theme
  cannot inject arbitrary rules through the token map.
- `components`/`motion` are **CSS only** — there is no JavaScript in themes.
- **Selector scope is enforced by a real CSS parser, not a regex.** Every
  selector must begin with `#root` and may only descend (child `>` or
  descendant space). Rejected: bare `html` / `body` / `:root`, sibling (`+`,
  `~`) combinators, `:has()`, and anything else that could escape `#root`.
  `:is()`, `:where()`, `:not()` and `:nth-child(... of …)` are validated
  recursively with the same rule.
- **Only `@media` and `@supports` at-rules are allowed** (their inner rules are
  validated recursively). `@keyframes` is allowed but the name **must use the
  `hd-` prefix** (e.g. `@keyframes hd-my-drift`); keyframe selectors are
  `from` / `to` / percentages. `@import`, `@font-face`, and any other at-rule
  are rejected.
- **No URL-bearing CSS.** `url(...)` (remote fonts, background URLs, any
  protocol) is rejected everywhere in `components`/`motion`.
- **Assets are confined to your theme's own `assets/` folder.** Only
  SVG / PNG / JPG / JPEG / WebP are allowed, each ≤ 512 KB, with magic-byte
  validation for PNG/JPEG/WebP. Absolute paths, `.`/`..`, backslashes,
  symlinks, and `../` escapes are rejected — a file cannot be read from any
  other theme or any user path.
- A theme never sees credentials, sessions, the shell, or the filesystem
  beyond its own `assets/` folder. Decorative assets are inlined as data URIs.
- Keep colors readable: prefer a readability overlay (in the Appearance
  window) and avoid very low-contrast `label-primary` vs `bg-base` pairs.

That's the whole model. Ship a folder, share it, and it loads anywhere DeepSeek Harness
Desktop runs.
