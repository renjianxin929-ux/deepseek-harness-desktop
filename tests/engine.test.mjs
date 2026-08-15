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
const { tokenRule } = sandbox.window.__HD_INTERNALS__;

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

console.log("engine.test.mjs: all assertions passed");
