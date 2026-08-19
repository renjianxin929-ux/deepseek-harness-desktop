// Narrow tests for the old-WKWebView Web Platform compatibility shim
// (src-tauri/src/web_compat.js). No browser needed: the shim runs in a VM with
// either the real Node AbortSignal/AbortController (native APIs present) or a
// minimal fake without AbortSignal.timeout/any (old-WKWebView simulation).
//
//   A. native timeout exists  -> not overridden
//   B. native any exists      -> not overridden
//   C. timeout missing        -> polyfill installed
//   D. timeout actually aborts
//   E. any missing            -> polyfill installed
//   F. any immediate-aborted input signal
//   G. any later-aborted input signal
//   H. reason propagation
//   I. listener cleanup (no leaks)
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import vm from "node:vm";
import assert from "node:assert";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const SHIM = readFileSync(join(root, "src-tauri", "src", "web_compat.js"), "utf8");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ── old-WKWebView simulation: AbortSignal/AbortController WITHOUT timeout/any ─
function makeFakeAbort() {
  class FakeSignal {
    constructor() {
      this.aborted = false;
      this.reason = undefined;
      this._listeners = {};
    }
    addEventListener(type, fn) {
      (this._listeners[type] = this._listeners[type] || []).push(fn);
    }
    removeEventListener(type, fn) {
      const a = this._listeners[type];
      if (!a) return;
      const i = a.indexOf(fn);
      if (i >= 0) a.splice(i, 1);
    }
    _emit(type, ev) {
      const a = (this._listeners[type] || []).slice();
      for (const fn of a) {
        try { fn.call(this, ev); } catch (_) { /* no-op */ }
      }
    }
  }
  class FakeController {
    constructor() { this.signal = new FakeSignal(); }
    abort(reason) {
      const s = this.signal;
      if (s.aborted) return;
      s.aborted = true;
      s.reason = arguments.length >= 1 ? reason : new DOMException("Aborted", "AbortError");
      s._emit("abort", { target: s, type: "abort" });
    }
  }
  class FakeAbortSignal {} // namespace object; no static timeout/any
  return { FakeSignal, FakeController, FakeAbortSignal };
}

function loadShim({ native }) {
  const sandbox = {
    console: { log() {}, error() {}, warn() {} },
    setTimeout,
    clearTimeout,
    DOMException,
  };
  if (native) {
    // Real Node AbortSignal/AbortController — both timeout and any exist.
    sandbox.AbortSignal = AbortSignal;
    sandbox.AbortController = AbortController;
  } else {
    const fake = makeFakeAbort();
    sandbox.AbortSignal = fake.FakeAbortSignal;
    sandbox.AbortController = fake.FakeController;
  }
  sandbox.window = {};
  vm.createContext(sandbox);
  vm.runInContext(SHIM, sandbox, { filename: "web_compat.js" });
  return sandbox;
}

// ── A + B: natives present -> preserved ─────────────────────────────────────
{
  const beforeTimeout = AbortSignal.timeout;
  const beforeAny = AbortSignal.any;
  const sb = loadShim({ native: true });
  assert.strictEqual(sb.window.__HD_WEB_COMPAT__.timeoutNative, true, "A: timeoutNative");
  assert.strictEqual(sb.window.__HD_WEB_COMPAT__.timeoutPolyfilled, false, "A: no timeout polyfill");
  assert.strictEqual(sb.AbortSignal.timeout, beforeTimeout, "A: native timeout preserved (same reference)");
  assert.strictEqual(sb.window.__HD_WEB_COMPAT__.anyNative, true, "B: anyNative");
  assert.strictEqual(sb.window.__HD_WEB_COMPAT__.anyPolyfilled, false, "B: no any polyfill");
  assert.strictEqual(sb.AbortSignal.any, beforeAny, "B: native any preserved (same reference)");
  console.log("web_compat.test.mjs: A+B (native preserved) passed");
}

// ── C + E: missing -> polyfilled ────────────────────────────────────────────
{
  const sb = loadShim({ native: false });
  const m = sb.window.__HD_WEB_COMPAT__;
  assert.strictEqual(m.timeoutNative, false, "C: timeout reported missing");
  assert.strictEqual(m.timeoutPolyfilled, true, "C: timeout polyfilled");
  assert.strictEqual(typeof sb.AbortSignal.timeout, "function", "C: AbortSignal.timeout installed");
  assert.strictEqual(m.anyNative, false, "E: any reported missing");
  assert.strictEqual(m.anyPolyfilled, true, "E: any polyfilled");
  assert.strictEqual(typeof sb.AbortSignal.any, "function", "E: AbortSignal.any installed");
  console.log("web_compat.test.mjs: C+E (polyfilled when missing) passed");
}

// ── D: timeout actually aborts with TimeoutError ────────────────────────────
{
  const sb = loadShim({ native: false });
  const t = sb.AbortSignal.timeout(15);
  assert.strictEqual(t.aborted, false, "D: not aborted before timeout");
  await sleep(60);
  assert.strictEqual(t.aborted, true, "D: aborted after timeout");
  assert.ok(t.reason, "D: has a reason");
  assert.strictEqual(t.reason.name, "TimeoutError", "D: reason is TimeoutError");
  console.log("web_compat.test.mjs: D (timeout aborts) passed");
}

// ── F: any() with an already-aborted input ──────────────────────────────────
{
  const sb = loadShim({ native: false });
  const c1 = new sb.AbortController();
  const c2 = new sb.AbortController();
  const reason = new sb.DOMException("already dead", "AbortError");
  c1.abort(reason);
  const composite = sb.AbortSignal.any([c1.signal, c2.signal]);
  assert.strictEqual(composite.aborted, true, "F: composite aborted immediately");
  assert.strictEqual(composite.reason, reason, "F: reason carried through");
  console.log("web_compat.test.mjs: F (immediate abort) passed");
}

// ── G: any() aborts when a later input aborts ───────────────────────────────
{
  const sb = loadShim({ native: false });
  const c1 = new sb.AbortController();
  const c2 = new sb.AbortController();
  const composite = sb.AbortSignal.any([c1.signal, c2.signal]);
  assert.strictEqual(composite.aborted, false, "G: not aborted initially");
  c2.abort(new sb.DOMException("c2 aborted", "AbortError"));
  assert.strictEqual(composite.aborted, true, "G: aborted when c2 aborts");
  console.log("web_compat.test.mjs: G (later abort) passed");
}

// ── H: reason propagation (first-aborted input's reason) ────────────────────
{
  const sb = loadShim({ native: false });
  const c1 = new sb.AbortController();
  const c2 = new sb.AbortController();
  const composite = sb.AbortSignal.any([c1.signal, c2.signal]);
  const customReason = { code: 42, message: "custom abort" };
  c2.abort(customReason);
  assert.strictEqual(composite.aborted, true, "H: aborted");
  assert.strictEqual(composite.reason, customReason, "H: reason propagated from c2");
  console.log("web_compat.test.mjs: H (reason propagation) passed");
}

// ── I: listener cleanup after composite abort ───────────────────────────────
{
  const sb = loadShim({ native: false });
  const c1 = new sb.AbortController();
  const c2 = new sb.AbortController();
  const composite = sb.AbortSignal.any([c1.signal, c2.signal]);
  assert.strictEqual((c1.signal._listeners.abort || []).length, 1, "I: c1 listener attached");
  assert.strictEqual((c2.signal._listeners.abort || []).length, 1, "I: c2 listener attached");
  c2.abort(new sb.DOMException("done", "AbortError"));
  assert.strictEqual((c1.signal._listeners.abort || []).length, 0, "I: c1 listener cleaned up");
  assert.strictEqual((c2.signal._listeners.abort || []).length, 0, "I: c2 listener cleaned up");
  // Aborting c1 afterwards must not throw or double-fire.
  c1.abort(new sb.DOMException("late", "AbortError"));
  assert.strictEqual(composite.aborted, true, "I: composite still aborted");
  console.log("web_compat.test.mjs: I (listener cleanup) passed");
}

console.log("web_compat.test.mjs: all assertions passed");
