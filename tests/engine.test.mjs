// Node-based tests for the injected appearance engine's security-critical
// helpers (run without a browser). Verifies that theme token overrides are
// strictly constrained to --custom-property declarations.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import vm from "node:vm";
import assert from "node:assert";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const engineSrc = readFileSync(join(root, "src-tauri", "src", "engine.js"), "utf8");

function loadEngine() {
  const sandbox = {
    console: { info() {}, error() {}, warn() {}, log() {} },
    setInterval() { return 0; },
    clearInterval() {},
    matchMedia() { return { matches: false }; },
  };
  sandbox.window = sandbox;
  sandbox.document = {
    readyState: "loading",
    addEventListener() {},
    getElementById() { return null; },
    querySelector() { return null; },
    createElement() { return { setAttribute() {}, appendChild() {}, style: {} }; },
    head: { appendChild() {} },
    documentElement: { setAttribute() {}, appendChild() {}, style: {} },
    body: { appendChild() {} },
  };
  vm.createContext(sandbox);
  vm.runInContext(engineSrc, sandbox, { filename: "engine.js" });
  return sandbox;
}

const sandbox = loadEngine();
const {
  tokenRule,
  depthT,
  depthKeyFactor,
  depthToBlur,
  depthToSaturate,
  depthToScrimFactor,
  depthToEdge,
  scaleColorAlpha,
  scaleSurfaceMap,
  shouldApplyMotion,
  scrimCss,
  glassMaterialCss,
  cinematicCss,
} = sandbox.window.__HD_INTERNALS__;

const near = (a, b, msg) => assert.ok(Math.abs(a - b) < 1e-9, `${msg}: ${a} !== ${b}`);

// Valid token overrides.
assert.strictEqual(
  tokenRule("body", { "--dsw-alias-bg-base": "rgb(1,2,3)" }),
  "body{--dsw-alias-bg-base:rgb(1,2,3) !important}"
);

// Dark selector.
assert.strictEqual(
  tokenRule("body[data-ds-dark-theme]", { "--x": "rgba(0,0,0,.5)" }),
  "body[data-ds-dark-theme]{--x:rgba(0,0,0,.5) !important}"
);

// Non custom-property keys are rejected (prevents arbitrary CSS injection).
assert.strictEqual(tokenRule("body", { "background": "url(x)" }), "");
assert.strictEqual(tokenRule("body", { "color": "red" }), "");

// Values that could terminate the declaration are rejected.
assert.strictEqual(tokenRule("body", { "--x": "red; background: url(evil)" }), "");
assert.strictEqual(tokenRule("body", { "--x": "red}" }), "");
assert.strictEqual(tokenRule("body", { "--x": "red{" }), "");

// Empty keys/values are skipped.
assert.strictEqual(tokenRule("body", { "": "x", "--x": "" }), "");

// Whitespace is trimmed and tolerated.
assert.strictEqual(
  tokenRule("body", { "  --x  ": "  red  " }),
  "body{--x:red !important}"
);

// ---------------------------------------------------------------------------
// Glass depth — CLEAR-GLASS semantics (per-key transparency; no global blur)
// ---------------------------------------------------------------------------
assert.strictEqual(depthT(0), 0);
assert.strictEqual(depthT(50), 0.5);
assert.strictEqual(depthT(100), 1);
assert.strictEqual(depthT(500), 1, "depth > 100 clamped");
assert.strictEqual(depthT(-5), 0, "negative depth clamped");

// Large surfaces clear out more with depth; composer/dialog stays readable.
near(depthKeyFactor("--dsw-alias-bg-base", 0), 1, "base @0 unchanged");
near(depthKeyFactor("--dsw-alias-bg-base", 1), 0.2, "base @100 clear");
near(depthKeyFactor("--dsw-specific-sidebar-fill", 1), 0.4, "sidebar @100 thin glass");
near(depthKeyFactor("--dsw-alias-bg-layer-3", 1), 0.93, "composer/dialog @100 stays readable");
near(depthKeyFactor("--dsw-alias-bg-layer-1", 1), 0.7, "cards @100");
assert.strictEqual(depthKeyFactor("--dsw-alias-brand-primary", 1), 1, "brand never scaled");

// Local blur is BOUNDED (not 18px, not depth-driven to a max).
assert.strictEqual(depthToBlur(0), 12);
assert.strictEqual(depthToBlur(100), 12, "blur bounded — depth 100 is NOT blurrier");
assert.strictEqual(depthToSaturate(0), 1);
assert.strictEqual(depthToSaturate(100), 1, "no global saturation");
assert.strictEqual(depthToScrimFactor(0), 1);
near(depthToScrimFactor(100), 0.35, "scrim reduced at depth 100");
assert.strictEqual(depthToEdge(0), 0.05);
near(depthToEdge(100), 0.20, "edge subtle, modest increase");

// Scrim is reduced by glass depth (scene stays vivid at high depth).
assert.ok(
  scrimCss({ overlay: 0.45, overlayMode: "dark" }, 1).includes("rgba(0,0,0,0.450)"),
  "full scrim at depth 0"
);
assert.ok(
  scrimCss({ overlay: 0.45, overlayMode: "dark" }, 0.35).includes("rgba(0,0,0,0.158)"),
  "scrim reduced at depth 100"
);

// Glass material: LOCAL composer blur only. #root / frame get NO blur.
assert.strictEqual(glassMaterialCss({ blur: 0, saturate: 1, edge: 0 }), "", "no glass at depth 0");
const materialCss = glassMaterialCss({ blur: 12, saturate: 1, edge: 0.2 });
assert.ok(materialCss.includes("@supports"), "local blur must be @supports-gated");
assert.ok(materialCss.includes("#root [contenteditable=\"true\"],#root textarea"), "blur targets composer only");
assert.ok(materialCss.includes("backdrop-filter:blur(12px)"), "bounded local blur");
assert.ok(!materialCss.includes("#root{"), "NO whole-#root glass rule");
assert.ok(!materialCss.includes("saturate("), "no saturation in glass material");
assert.ok(materialCss.includes("box-shadow:inset 0 1px 0 rgba(255,255,255,0.200)"), "edge highlight");

// Cinematic motion: slow scale drift on the media wrapper only, reduced-motion guarded.
const cinematic = cinematicCss();
assert.ok(cinematic.includes("@keyframes hd-cinematic-drift"), "scale-drift keyframes present");
assert.ok(cinematic.includes("scale(1.04)"), "scale drift reaches 1.04");
assert.ok(cinematic.includes("#hd-cinematic"), "motion targets the media wrapper only");
assert.ok(cinematic.includes("prefers-reduced-motion"), "reduced-motion guard present");

// ---------------------------------------------------------------------------
// Glass depth — scaleColorAlpha
// ---------------------------------------------------------------------------
assert.strictEqual(
  scaleColorAlpha("rgba(243,247,250,0.92)", 1),
  "rgba(243,247,250,0.920)"
);
assert.strictEqual(
  scaleColorAlpha("rgba(243,247,250,0.92)", 0.6),
  "rgba(243,247,250,0.552)"
);
// Opaque rgb() is left alone when factor >= 1 (nothing to deepen).
assert.strictEqual(scaleColorAlpha("rgb(255,255,255)", 1.2), "rgb(255,255,255)");
// Opaque rgb() gains alpha when deepened.
assert.strictEqual(scaleColorAlpha("rgb(255,255,255)", 0.6), "rgba(255,255,255,0.600)");
// Percent components in rgb() are preserved; only alpha is added.
assert.strictEqual(scaleColorAlpha("rgb(100%, 0%, 0%)", 0.6), "rgba(100%,0%,0%,0.600)");
// Alpha is clamped to a near-transparent floor + opaque ceiling.
assert.strictEqual(scaleColorAlpha("rgba(255,255,255,0.95)", 1.4), "rgba(255,255,255,0.980)");
assert.strictEqual(scaleColorAlpha("rgba(255,255,255,0.2)", 0.2), "rgba(255,255,255,0.040)");
// Unparsable values are returned untouched (fail-safe).
assert.strictEqual(scaleColorAlpha("not-a-color", 0.5), "not-a-color");

// ---------------------------------------------------------------------------
// Glass depth — scaleSurfaceMap (per-key scaling; only surface keys scaled)
// ---------------------------------------------------------------------------
const scaled = scaleSurfaceMap(
  {
    "--dsw-alias-bg-base": "rgba(21,21,23,0.50)",
    "--dsw-specific-sidebar-fill": "rgba(21,21,23,0.50)",
    "--dsw-alias-bg-layer-3": "rgba(21,21,23,0.50)",
    "--dsw-alias-brand-primary": "rgb(77,124,254)",
  },
  1.0
);
assert.strictEqual(scaled["--dsw-alias-bg-base"], "rgba(21,21,23,0.100)", "main canvas clears out @100");
assert.strictEqual(scaled["--dsw-specific-sidebar-fill"], "rgba(21,21,23,0.200)", "sidebar thin glass @100");
assert.strictEqual(scaled["--dsw-alias-bg-layer-3"], "rgba(21,21,23,0.465)", "composer stays readable @100");
assert.strictEqual(scaled["--dsw-alias-brand-primary"], "rgb(77,124,254)", "brand token never scaled");

// ---------------------------------------------------------------------------
// Motion — shouldApplyMotion (reduced-motion + toggle gating)
// ---------------------------------------------------------------------------
assert.strictEqual(shouldApplyMotion(true, false, "x"), true);
assert.strictEqual(shouldApplyMotion(true, true, "x"), false, "reduce-motion must disable motion");
assert.strictEqual(shouldApplyMotion(false, false, "x"), false, "motion toggle off must disable motion");
assert.strictEqual(shouldApplyMotion(true, false, ""), false, "empty motion must not apply");
assert.strictEqual(shouldApplyMotion(true, false, null), false);

console.log("engine.test.mjs: all assertions passed");
