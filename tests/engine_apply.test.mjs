// Node-based test that drives the ENGINE's real applyAppearance() path with a
// complete-enough fake DOM. Unit tests only cover pure helpers; this exercises
// the DOM-dependent apply flow (ensureBackdrop/renderImage/renderVideo) that the
// Human E2E found broken, and proves the engine receives + records the config.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import vm from "node:vm";
import assert from "node:assert";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const engineSrc = readFileSync(join(root, "src-tauri", "src", "engine.js"), "utf8");

function makeSandbox() {
  const byId = {};

  function makeEl(tag) {
    const el = {
      tagName: String(tag || "div").toUpperCase(),
      _id: "",
      _src: "",
      _text: "",
      style: {},
      attrs: {},
      children: [],
      parentNode: null,
      childElementCount: 0,
      setAttribute(k, v) { this.attrs[k] = String(v); },
      getAttribute(k) { return Object.prototype.hasOwnProperty.call(this.attrs, k) ? this.attrs[k] : null; },
      removeAttribute(k) { delete this.attrs[k]; },
      appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
      removeChild(c) { const i = this.children.indexOf(c); if (i >= 0) { this.children.splice(i, 1); c.parentNode = null; } return c; },
      addEventListener() {},
      classList: { add() {}, remove() {}, contains() { return false; } },
      pause() {},
      play() { return Promise.resolve(); },
      get src() { return this._src; },
      set src(v) { this._src = String(v); },
      get display() { return this.style.display; },
      set display(v) { this.style.display = v; },
    };
    Object.defineProperty(el, "id", {
      get() { return this._id; },
      set(v) { this._id = String(v); if (this._id) byId[this._id] = el; },
    });
    Object.defineProperty(el, "textContent", {
      get() { return this._text; },
      set(v) { this._text = String(v); },
    });
    return el;
  }

  const rootEl = makeEl("div");
  rootEl.id = "root";
  rootEl.childElementCount = 1;
  rootEl._text = "mounted";

  const sandbox = {
    console: { info() {}, error() {}, warn() {}, log() {} },
    Image: function () { return {}; },
    setInterval(cb) { cb(); return 1; },
    clearInterval() {},
    setTimeout() { return 0; },
    clearTimeout() {},
    matchMedia() { return { matches: false }; },
    getComputedStyle() { return { getPropertyValue() { return "rgba(0,0,0,1)"; } }; },
  };
  sandbox.window = sandbox;
  sandbox.document = {
    readyState: "loading",
    addEventListener() {},
    getElementById(id) { return byId[id] || null; },
    createElement(tag) { return makeEl(tag); },
    querySelector(sel) { return sel === "#root" ? rootEl : null; },
    querySelectorAll() { return []; },
    head: { appendChild() {} },
    documentElement: { setAttribute() {}, appendChild() {}, style: {} },
    body: { appendChild() {}, nodeType: 1 },
  };
  sandbox.__byId = byId;
  vm.createContext(sandbox);
  vm.runInContext(engineSrc, sandbox, { filename: "engine.js" });
  return sandbox;
}

const deepGlassPayload = {
  compatMode: "normal",
  motionEnabled: true,
  glassDepth: 100,
  theme: {
    id: "deep-glass",
    tokens: {
      light: { "--dsw-alias-bg-base": "rgba(250,251,253,0.68)" },
      dark: { "--dsw-alias-bg-base": "rgba(20,20,24,0.74)" },
    },
    surfaces: {
      light: { "--dsw-alias-bg-base": "rgba(250,251,253,0.55)" },
      dark: { "--dsw-alias-bg-base": "rgba(18,18,22,0.46)" },
    },
    components: "#root [contenteditable=\"true\"] { border-radius: 16px; }",
    motion: "@keyframes hd-x { from { opacity: 0; } to { opacity: 1; } }",
    asset: "linear-gradient(155deg, rgba(70,76,92,0.26), rgba(18,20,28,0.40))",
  },
  wallpaper: {
    active: true,
    url: "hd-wallpaper://current?v=1",
    mediaType: "image",
    fit: "cover",
    position: "center",
    opacity: 0.6,
    blur: 0,
    overlay: 0.45,
    overlayMode: "auto",
  },
};

{
  const sandbox = makeSandbox();
  sandbox.window.__HD_APPLY__(deepGlassPayload);
  const state = sandbox.window.__HD_STATE__;
  assert.ok(state, "__HD_STATE__ must exist");
  assert.strictEqual(state.error ?? null, null, `apply threw: ${state.error}`);
  assert.strictEqual(state.level, "compatible");
  assert.strictEqual(state.theme, "deep-glass");
  assert.strictEqual(state.glassDepth, 100);
  assert.strictEqual(state.wallpaperActive, true);
  assert.strictEqual(state.mediaType, "image");
  // Backdrop + wallpaper element must have been created and styled.
  const wall = sandbox.__byId["hd-wallpaper"];
  assert.ok(wall, "hd-wallpaper element must be created");
  assert.ok(wall.style.cssText.includes("hd-wallpaper://current?v=1"), "wallpaper URL applied");
  const glass = sandbox.__byId["hd-glass"];
  assert.ok(glass, "hd-glass style must be created at depth 100");
  assert.ok(glass.textContent.includes("backdrop-filter"), "local composer blur applied");
  assert.ok(!glass.textContent.includes("#root{"), "NO whole-viewport #root blur");
  assert.ok(
    glass.textContent.includes("#root [contenteditable=\"true\"]"),
    "blur targets the composer, not the frame"
  );
  console.log("engine_apply.test.mjs (image): assertions passed");
}

{
  // Video path must also apply without throwing and record mediaType=video.
  const sandbox = makeSandbox();
  const videoPayload = JSON.parse(JSON.stringify(deepGlassPayload));
  videoPayload.wallpaper.mediaType = "video";
  sandbox.window.__HD_APPLY__(videoPayload);
  const state = sandbox.window.__HD_STATE__;
  assert.strictEqual(state.error ?? null, null, `video apply threw: ${state.error}`);
  assert.strictEqual(state.mediaType, "video");
  const video = sandbox.__byId["hd-video"];
  assert.ok(video, "hd-video element must be created");
  assert.strictEqual(video.attrs.muted, "", "video must be muted");
  assert.strictEqual(video.attrs.loop, "", "video must loop");
  assert.strictEqual(video.attrs.autoplay, "", "video must autoplay");
  assert.strictEqual(video.attrs.playsinline, "", "video must play inline");
  assert.strictEqual(video._src, "hd-wallpaper://current?v=1", "video src set to controlled URL");
  // Video supplies its own motion: no extra cinematic pan/zoom style must be injected.
  assert.ok(!sandbox.__byId["hd-cinematic-style"], "video must NOT get cinematic transform");
  console.log("engine_apply.test.mjs (video): assertions passed");
}

console.log("engine_apply.test.mjs: all assertions passed");
