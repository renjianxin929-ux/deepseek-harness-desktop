// DeepSeek Harness Desktop — stale session guard (fail-safe only).
//
// Injected alongside the appearance engine into the official DeepSeek Harness
// Web UI. The WebView's client state can drift from the backend's session
// truth: a long-running turn may finish on the backend (or fail) while the UI
// keeps showing "Deep diving..." and a Stop button. This guard adds one
// minimal reconciliation fail-safe and nothing else.
//
// Behavior:
//   * observes the client's running state (the composer Stop control) ONLY —
//     it never tries to infer progress from DOM mutations, so cosmetic timer
//     ticks or continuous DOM churn cannot suppress a reconciliation,
//   * while the client shows a turn running, polls the backend's authoritative
//     state every POLL_INTERVAL_MS via the official read-only
//     `POST /api/session.list` (DOM freshness is NOT a precondition),
//   * reconciles against the CURRENT session only (the official persisted
//     selection under localStorage["dsh.sessions.current"]), so a completed
//     session A still reconciles even while unrelated sessions B/C are running,
//   * reloads the official page ONLY when the backend confirms the current
//     session is not running while the client still believes it is, and
//   * shows a non-destructive banner when the backend is unreachable.
//
// Hard guarantees: this script NEVER submits a prompt, NEVER cancels a turn,
// NEVER stops or restarts the harness process, and NEVER creates a session.
// It has no access to Tauri IPC. Its only side effects are a read-only
// `session.list` query and, at most, a same-origin page reload.
(function () {
  "use strict";

  var ENGINE_VERSION = 2;

  // How often, while the client shows a turn running, to re-read the current
  // session's authoritative backend state.
  var POLL_INTERVAL_MS = 30000;
  var QUERY_TIMEOUT_MS = 8000;

  // The composer renders a stop square (<svg><rect x=3 y=3 width=10 height=10
  // rx=3/>) while a turn is running, and a send arrow (<path>) while idle. The
  // square is the structural, locale-independent "client believes running"
  // signal.
  var STOP_RECT_SELECTOR = 'svg rect[width="10"][height="10"]';

  // The official client persists the currently-selected session id under this
  // exact localStorage key (the durable half of its `list.current`). Reading it
  // is the official session-selection path — never a guess from title, DOM text,
  // or list position.
  var SESSION_SELECTION_KEY = "dsh.sessions.current";

  var IDS = {
    overlay: "hd-session-guard-overlay",
    style: "hd-session-guard-style",
  };

  var booted = false;
  var bootTimer = null;
  var heartbeat = null;
  var reconciled = false;

  function log() {
    try {
      if (window.console && console.info) {
        console.info.apply(console, ["[DeepSeek Harness Desktop session guard]"].concat(
          Array.prototype.slice.call(arguments)
        ));
      }
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

  function rootEl() {
    return document.getElementById("root");
  }

  function rootMounted() {
    var root = rootEl();
    if (!root) return false;
    return root.childElementCount > 0 || (root.textContent || "").trim().length > 0;
  }

  // -------------------------------------------------------------------------
  // Pure decision helpers (no DOM, no network — exercised by unit tests).
  // -------------------------------------------------------------------------

  // A backend read is warranted whenever the client believes it is running.
  // DOM freshness is deliberately NOT a factor: cosmetic clocks and continuous
  // DOM mutations must never suppress a reconciliation.
  function shouldQueryBackend(clientRunning) {
    return !!clientRunning;
  }

  // Map (client belief, per-current-session backend truth) to one action.
  //   "noop"      — nothing to do (idle, current session genuinely running,
  //                 or current session unknown)
  //   "reconcile" — backend says the CURRENT session is not running while the
  //                 client still shows it running
  //   "reconnect" — backend truth is unreadable; show the re-syncing banner
  function resolveDecision(clientRunning, backend) {
    if (!shouldQueryBackend(clientRunning)) {
      return "noop";
    }
    if (!backend || backend.reachable !== true) {
      return "reconnect";
    }
    if (backend.currentRunning === true) {
      return "noop";
    }
    if (backend.currentRunning === false) {
      return "reconcile";
    }
    // currentRunning unknown (could not determine the current session): do
    // nothing — never reload on uncertainty.
    return "noop";
  }

  // Resolve the currently-selected session id from the official persisted
  // selection. Returns null when the selection cannot be read (the guard then
  // refuses to reconcile rather than guess).
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

  // Parse the official `session.list` server-response envelope and reduce it to
  // the CURRENT session's running state: `{ reachable, currentRunning }` where
  // currentRunning is true/false when the current session was found, and null
  // when the current id is unknown. A malformed or failed envelope is treated
  // as "unreadable" (never as "not running") so the guard never reconciles on a
  // guess. An absent current session is "not running" (stale/removed).
  function parseSessionList(json, currentId) {
    var items = null;
    try {
      if (
        json &&
        json.result &&
        json.result.ok === true &&
        json.result.value &&
        Array.isArray(json.result.value.items)
      ) {
        items = json.result.value.items;
      }
    } catch (_) {
      return { reachable: false, currentRunning: null };
    }
    if (items === null) return { reachable: false, currentRunning: null };
    if (!currentId) return { reachable: true, currentRunning: null };
    for (var i = 0; i < items.length; i++) {
      var item = items[i];
      if (item && item.sessionId === currentId) {
        return { reachable: true, currentRunning: item.running === true };
      }
    }
    return { reachable: true, currentRunning: false };
  }

  // A button is a Stop control when it is enabled and contains the stop square.
  function isStopControl(el) {
    if (!el) return false;
    try {
      if (el.disabled === true) return false;
      return !!el.querySelector && !!el.querySelector(STOP_RECT_SELECTOR);
    } catch (_) {
      return false;
    }
  }

  function isVisible(el) {
    try {
      if (typeof el.getClientRects !== "function") return true;
      return el.getClientRects().length > 0;
    } catch (_) {
      return true;
    }
  }

  // The client believes it is running while any enabled, visible button holds
  // the composer's stop square.
  function detectRunning() {
    var buttons;
    try {
      buttons = document.querySelectorAll("button");
    } catch (_) {
      return false;
    }
    for (var i = 0; i < buttons.length; i++) {
      if (isStopControl(buttons[i]) && isVisible(buttons[i])) return true;
    }
    return false;
  }

  // -------------------------------------------------------------------------
  // Backend truth + recovery
  // -------------------------------------------------------------------------

  function uuid() {
    try {
      if (window.crypto && typeof crypto.randomUUID === "function") {
        return crypto.randomUUID();
      }
    } catch (_) { /* ignore */ }
    return "hd-" + Date.now().toString(36) + "-" +
      Math.random().toString(36).slice(2, 12);
  }

  // Read-only query of the official session-list endpoint. Any transport or
  // parse failure resolves to `{ reachable: false }` so the caller can only
  // ever fall back to the re-syncing banner (never to a destructive action).
  function queryBackend() {
    var currentId = currentSessionId();
    var controller = null;
    var timeoutId = null;
    try {
      if (typeof AbortController !== "undefined") {
        controller = new AbortController();
        timeoutId = setTimeout(function () { controller.abort(); }, QUERY_TIMEOUT_MS);
      }
    } catch (_) { /* ignore */ }

    var options = {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        type: "client-request",
        rpcId: uuid(),
        method: "session.list",
        payload: {},
      }),
    };
    if (controller) options.signal = controller.signal;

    return fetch("/api/session.list", options)
      .then(function (response) {
        if (!response.ok) throw new Error("http " + response.status);
        return response.json();
      })
      .then(function (json) {
        return parseSessionList(json, currentId);
      })
      .catch(function () {
        return { reachable: false, currentRunning: null };
      })
      .then(function (backend) {
        if (timeoutId) clearTimeout(timeoutId);
        return backend;
      });
  }

  function reconcile() {
    if (reconciled) return;
    reconciled = true;
    if (window.__HD_GUARD_STATE__) window.__HD_GUARD_STATE__.reconciled = true;
    log("reconcile: backend reports current session not running; reloading official page");
    try {
      location.reload();
    } catch (e) {
      log("reload failed", e);
    }
  }

  function overlayMessage() {
    var lang = "";
    try {
      lang = (document.documentElement.lang || "") ||
        (navigator.language || "") ||
        (navigator.languages && navigator.languages[0]) ||
        "";
    } catch (_) { /* ignore */ }
    return /^zh/i.test(lang)
      ? "连接暂时中断，正在重新同步任务状态…"
      : "Connection interrupted. Re-syncing task state…";
  }

  function ensureStyle() {
    if (document.getElementById(IDS.style)) return;
    var node = document.createElement("style");
    node.id = IDS.style;
    node.textContent =
      "#hd-session-guard-overlay{position:fixed;top:12px;left:50%;" +
      "transform:translateX(-50%);z-index:2147483000;max-width:min(92vw,520px);" +
      "padding:10px 16px;border-radius:10px;border:1px solid rgba(255,196,0,.55);" +
      "background:rgba(28,28,32,.92);color:rgba(255,255,255,.95);" +
      "font:500 13px/1.4 -apple-system,'SF Pro Text',sans-serif;" +
      "box-shadow:0 6px 20px rgba(0,0,0,.3);text-align:center;pointer-events:none;" +
      "backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);}";
    (document.head || document.documentElement).appendChild(node);
  }

  function showOverlay() {
    var node = document.getElementById(IDS.overlay);
    if (!node) {
      ensureStyle();
      node = document.createElement("div");
      node.id = IDS.overlay;
      node.setAttribute("role", "status");
      node.setAttribute("data-hd-session-guard", "true");
      node.textContent = overlayMessage();
      (document.body || document.documentElement).appendChild(node);
    }
  }

  function clearOverlay() {
    var node = document.getElementById(IDS.overlay);
    if (node && node.parentNode) node.parentNode.removeChild(node);
  }

  // -------------------------------------------------------------------------
  // Heartbeat
  // -------------------------------------------------------------------------

  function tick() {
    if (!booted || reconciled || !isHarnessPage()) return;
    var running = detectRunning();

    if (!running) {
      clearOverlay();
      return;
    }

    queryBackend().then(function (backend) {
      if (reconciled) return;
      var decision = resolveDecision(running, backend);
      if (decision === "reconcile") {
        reconcile();
      } else if (decision === "reconnect") {
        showOverlay();
      } else {
        clearOverlay();
      }
    });
  }

  // -------------------------------------------------------------------------
  // Boot (only on the Harness page, after the official UI has mounted)
  // -------------------------------------------------------------------------

  function scheduleBoot() {
    if (bootTimer) return;
    var attempts = 0;
    var maxAttempts = 300; // ~30s at 100ms
    bootTimer = setInterval(function () {
      attempts++;
      if (rootMounted()) {
        clearInterval(bootTimer);
        bootTimer = null;
        boot();
      } else if (attempts >= maxAttempts) {
        clearInterval(bootTimer);
        bootTimer = null;
      }
    }, 100);
  }

  function boot() {
    if (booted) return;
    if (!isHarnessPage()) return; // loading page (tauri://) — not our concern
    if (!rootMounted()) {
      scheduleBoot();
      return;
    }
    booted = true;
    if (window.__HD_GUARD_STATE__) window.__HD_GUARD_STATE__.booted = true;
    heartbeat = setInterval(tick, POLL_INTERVAL_MS);
  }

  // Pure helpers exposed for diagnostics and automated tests (no privileges).
  window.__HD_GUARD_INTERNALS__ = {
    POLL_INTERVAL_MS: POLL_INTERVAL_MS,
    QUERY_TIMEOUT_MS: QUERY_TIMEOUT_MS,
    STOP_RECT_SELECTOR: STOP_RECT_SELECTOR,
    SESSION_SELECTION_KEY: SESSION_SELECTION_KEY,
    shouldQueryBackend: shouldQueryBackend,
    resolveDecision: resolveDecision,
    currentSessionId: currentSessionId,
    parseSessionList: parseSessionList,
    isStopControl: isStopControl,
  };

  window.__HD_GUARD_STATE__ = {
    version: ENGINE_VERSION,
    booted: false,
    reconciled: false,
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
