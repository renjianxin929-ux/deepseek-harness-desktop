# Deep Glass — official glass preset

Deep Glass is a **built-in** theme shipped with the app. It is a real
`theme.json`, not a hard-coded DOM hack: every effect here is expressible in the
same declarative model as [`../ocean/`](../ocean/) and
[`../starter/`](../starter/).

## Design intent

- **Foreground HUD over background.** The Harness UI reads as a translucent glass
  layer sitting on top of your wallpaper, not an opaque app with a hidden
  wallpaper.
- **Maximum background visibility.** The wallpaper stays the star; surfaces are
  translucent tint, not opaque paint.
- **Still recognizably Harness.** Layout, typography, brand accent and text
  colors are untouched; only surface translucency, ambient depth and motion
  change.
- **Readable for long sessions.** The surface hierarchy keeps the sidebar most
  translucent, main surfaces highly translucent, secondary panels medium, and the
  composer / code / dialogs (layer-3) the least translucent. A restrained
  readability overlay is retained for arbitrary wallpaper contrast.

## Surface hierarchy (dark values, for illustration)

| Surface | Token | Dark alpha |
| --- | --- | --- |
| Sidebar | `--dsw-specific-sidebar-fill` | 0.36 (most transparent) |
| App base / main surfaces | `--dsw-alias-bg-base` | 0.46 |
| Secondary panels | `--dsw-alias-bg-module-platform` | 0.58 |
| Elevated surfaces | `--dsw-alias-bg-layer-1` / `-2` | 0.54 / 0.62 |
| Composer / code / dialogs | `--dsw-alias-bg-layer-3` | 0.76 (least transparent) |

`tokens` (no wallpaper) are a touch more opaque than `surfaces` (wallpaper
active) so text stays readable when the background is the ambient gradient
rather than an image.

## Motion

`motion` applies one slow `hd-deep-glass-pan` keyframe (~30 s per sweep) to the
ambient layer's `background-position`, with a `prefers-reduced-motion` guard.
The engine additionally suppresses motion entirely under Reduce Motion and the
Appearance ▸ Motion toggle.

## What's in `theme.json`

| Section | Role |
| --- | --- |
| `tokens.light` / `tokens.dark` | translucent surface tokens (no wallpaper) |
| `surfaces.light` / `surfaces.dark` | deeper translucent surfaces (wallpaper active) |
| `components` | readable composer/code surfaces, focus ring, selection, restrained highlight |
| `motion` | slow ambient pan drift + reduced-motion guard |
| `asset` | a subtle neutral ambient gradient (depth, no forced color cast) |
