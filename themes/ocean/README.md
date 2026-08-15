# Ocean — showcase theme

Ocean is the polished showcase for the DeepSeek Harness Desktop appearance system. It is
an *example of the system*, not the architecture itself — every effect here is
expressible in a plain `theme.json` (see [`../starter/README.md`](../starter/README.md)).

## Design intent

- **Still recognizably Harness.** Ocean keeps the official layout, typography,
  and behavior untouched; it only shifts the palette and adds a quiet ambient
  layer.
- **Restrained, not decorative.** A cool ocean-tinted neutral ramp, a sea-blue
  accent, and one faint ambient gradient with the original DeepSeek whale
  silhouette (unaltered) in the corner.
- **Subtle motion.** A slow 22s ambient drift, automatically disabled under
  Reduce Motion and the Motion toggle.

## What's in `theme.json`

| Section | Role |
| --- | --- |
| `tokens.light` / `tokens.dark` | ocean-tinted `--dsw-alias-*` overrides |
| `surfaces` | translucent panels for the wallpaper glass effect |
| `components` | rounded composer + ocean focus ring + tinted selection |
| `motion` | a single slow `hd-ocean-drift` keyframe on `#hd-ambient` |
| `asset` | a base64 SVG gradient + the original DeepSeek whale mark |

`assets/whale.svg` is the human-readable source for the `asset` data URI; it is
kept in the repo for transparency and is regenerated into `theme.json` when the
theme is edited (see `scripts/gen_theme_assets.js` if you add one).
