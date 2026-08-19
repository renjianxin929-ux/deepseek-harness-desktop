// DeepSeek Harness Desktop — Web Platform compatibility shim (old WKWebView).
//
// WHY: the bundled Harness rc.7 UI uses `AbortSignal.timeout()` and
// `AbortSignal.any()` in its bounded-unary request path (e.g. the
// dsh-host-apiproxy fetch carrier's `postJson`: `signal === undefined
// ? AbortSignal.timeout(this.timeoutMs) : AbortSignal.any([...])`, and the same
// pattern in dsh-client-connection, dsh-llm-retry, dsh-agent-loop, ...).
// Settings RPC (models / agent presets / permissions) runs through that path.
// macOS Monterey's WKWebView does not provide `AbortSignal.timeout`, so those
// calls throw `AbortSignal.timeout is not a function` and the Settings pages
// fail to load.
//
// This file is the Desktop-controlled initialization layer: it is injected at
// document start, BEFORE the Harness module bundle executes. It only installs
// standards-compatible fallbacks when the native APIs are MISSING; when native
// `AbortSignal.timeout` / `AbortSignal.any` exist they are preserved untouched.
// It does not modify fetch, Promise, or any other global; no third-party
// dependency; no filesystem/network access; exposes no sensitive state.
//
// Observability (non-sensitive): window.__HD_WEB_COMPAT__ =
// { timeoutNative, timeoutPolyfilled, anyNative, anyPolyfilled }.
(function () {
  "use strict";
  var MARK = (window.__HD_WEB_COMPAT__ = window.__HD_WEB_COMPAT__ || {});
  MARK.timeoutNative = false;
  MARK.timeoutPolyfilled = false;
  MARK.anyNative = false;
  MARK.anyPolyfilled = false;

  var canPolyfill = typeof AbortController === "function";
  var hasAbortSignal = typeof AbortSignal === "object" || typeof AbortSignal === "function";

  function timeoutReason(ms) {
    // Standards-compatible TimeoutError reason when the platform can build one.
    if (typeof DOMException === "function") {
      try {
        return new DOMException(
          "The operation was aborted due to timeout after " + ms + " ms.",
          "TimeoutError"
        );
      } catch (_) { /* fall through to plain Error */ }
    }
    var err = new Error("The operation was aborted due to timeout after " + ms + " ms.");
    err.name = "TimeoutError";
    return err;
  }

  if (hasAbortSignal && canPolyfill) {
    // ── AbortSignal.timeout(ms) ───────────────────────────────────────────
    // Returns an AbortSignal that aborts after ms with a TimeoutError reason.
    // Installed ONLY when the native API is missing.
    MARK.timeoutNative = typeof AbortSignal.timeout === "function";
    if (!MARK.timeoutNative) {
      AbortSignal.timeout = function (ms) {
        var n = Number(ms);
        if (!(n >= 0) || !Number.isFinite(n)) n = 0; // NaN/negative/Infinity -> 0
        var controller = new AbortController();
        var reason = timeoutReason(n);
        var timer = setTimeout(function () {
          try {
            controller.abort(reason);
          } catch (_) {
            try { controller.abort(); } catch (__) { /* no-op */ }
          }
        }, n);
        // Clear the timer once the signal aborts for any reason: no leaked timers.
        try {
          controller.signal.addEventListener(
            "abort",
            function () { clearTimeout(timer); },
            { once: true }
          );
        } catch (_) { /* environment without addEventListener options */ }
        return controller.signal;
      };
      MARK.timeoutPolyfilled = true;
    }

    // ── AbortSignal.any(signals) ──────────────────────────────────────────
    // Composite signal that aborts (with the first input's reason) as soon as
    // ANY input signal aborts. Installed ONLY when the native API is missing.
    MARK.anyNative = typeof AbortSignal.any === "function";
    if (!MARK.anyNative) {
      AbortSignal.any = function (signals) {
        var list = Array.isArray(signals) ? signals : [];
        var controller = new AbortController();
        var cleanups = [];
        var done = false;

        function abortWithReason(sig) {
          var r = sig && typeof sig.reason !== "undefined" ? sig.reason : undefined;
          if (r !== undefined) {
            try { controller.abort(r); return; } catch (_) { /* fall back */ }
          }
          try { controller.abort(); } catch (_) { /* no-op */ }
        }
        function cleanupAll() {
          if (done) return;
          done = true;
          for (var i = 0; i < cleanups.length; i++) {
            try { cleanups[i](); } catch (_) { /* no-op */ }
          }
          cleanups.length = 0;
        }

        for (var i = 0; i < list.length; i++) {
          var sig = list[i];
          if (!sig || typeof sig.addEventListener !== "function") continue;
          if (sig.aborted) {
            // Already-aborted input: the composite aborts immediately with its reason.
            abortWithReason(sig);
            cleanupAll();
            return controller.signal;
          }
          var onAbort = function () {
            abortWithReason(sig);
            cleanupAll();
          };
          try { sig.addEventListener("abort", onAbort); } catch (_) { /* no-op */ }
          (function (target, handler) {
            cleanups.push(function () {
              try { target.removeEventListener("abort", handler); } catch (_) { /* no-op */ }
            });
          })(sig, onAbort);
        }
        return controller.signal;
      };
      MARK.anyPolyfilled = true;
    }
  }
})();
