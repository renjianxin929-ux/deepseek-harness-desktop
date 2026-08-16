// DeepSeek Harness Desktop — current-session beacon (read-only).
//
// Injected alongside the appearance engine and session guard into the official
// Harness Web UI. Its only job is to report the officially-persisted current
// session id to the Rust backend so the read-only Usage surface can show the
// CURRENT session without guessing from timestamps.
//
// Hard guarantees: this script NEVER submits, cancels, creates, or renames a
// session, never reaches Tauri IPC, and never mutates anything. Its only side
// effects are reading localStorage["dsh.sessions.current"] and firing a
// read-only `hd-usage-session://` image beacon (same pattern as the existing
// appearance beacon).
(function () {
  "use strict";

  var SESSION_SELECTION_KEY = "dsh.sessions.current";
  var REPORT_INTERVAL_MS = 30000;

  // Resolve the currently-selected session id from the official persisted
  // selection (the durable half of the client's `list.current`). Returns null
  // when the selection cannot be read.
  function currentSessionId() {
    try {
      if (typeof localStorage === "undefined") return null;
      var raw = localStorage.getItem(SESSION_SELECTION_KEY);
      if (!raw) return null;
      var sel = JSON.parse(raw);
      if (!sel || typeof sel !== "object") return null;
      var child = sel.subagentAddress && sel.subagentAddress.childSessionId;
      if (typeof child === "string" && child) return child;
      if (typeof sel.sessionId === "string" && sel.sessionId) return sel.sessionId;
      return null;
    } catch (_) {
      return null;
    }
  }

  function report() {
    try {
      var id = currentSessionId();
      if (!id) return;
      var img = new Image();
      img.src = "hd-usage-session://set?id=" + encodeURIComponent(id);
    } catch (_) { /* ignore */ }
  }

  function isHarnessPage() {
    try {
      return (
        location.protocol === "http:" &&
        (location.hostname === "127.0.0.1" || location.hostname === "localhost")
      );
    } catch (_) {
      return false;
    }
  }

  function rootMounted() {
    var root = document.getElementById("root");
    if (!root) return false;
    return root.childElementCount > 0 || (root.textContent || "").trim().length > 0;
  }

  var booted = false;
  var bootTimer = null;
  var pollTimer = null;

  function boot() {
    if (booted) return;
    if (!isHarnessPage()) return;
    if (!rootMounted()) {
      scheduleBoot();
      return;
    }
    booted = true;
    report();
    pollTimer = setInterval(report, REPORT_INTERVAL_MS);
  }

  function scheduleBoot() {
    if (bootTimer) return;
    var attempts = 0;
    bootTimer = setInterval(function () {
      attempts++;
      if (rootMounted()) {
        clearInterval(bootTimer);
        bootTimer = null;
        boot();
      } else if (attempts >= 300) {
        clearInterval(bootTimer);
        bootTimer = null;
      }
    }, 100);
  }

  // Diagnostics only (no privileges).
  window.__HD_USAGE_BEACON__ = { currentSessionId: currentSessionId };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
